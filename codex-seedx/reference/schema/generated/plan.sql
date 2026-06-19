ALTER TABLE snap_flows
    ADD COLUMN last_published timestamptz;

CREATE INDEX CONCURRENTLY IF NOT EXISTS index_snap_flows_edition ON snap_flows (flow_id, last_published, version);

-- pgschema:wait
SELECT
    COALESCE(i.indisvalid, FALSE) AS done,
    CASE WHEN p.blocks_total > 0 THEN
        p.blocks_done * 100 / p.blocks_total
    ELSE
        0
    END AS progress
FROM
    pg_class c
    LEFT JOIN pg_index i ON c.oid = i.indexrelid
    LEFT JOIN pg_stat_progress_create_index p ON c.oid = p.index_relid
WHERE
    c.relname = 'index_snap_flows_edition';

DROP VIEW IF EXISTS flow_history RESTRICT;

CREATE OR REPLACE VIEW flow_history AS
SELECT
    flow.flow_id AS id,
    flow.version,
    (((dense_rank() OVER (PARTITION BY flow.flow_id ORDER BY flow.last_published NULLS FIRST) - 1)::text) || '.'::text) || ((row_number() OVER (PARTITION BY flow.flow_id, flow.last_published ORDER BY flow.version) - 1)::text) AS release,
    flow.identity_project_id,
    flow.occurred_at,
    jsonb_build_object('id', flow.flow_id, 'identity_project_id', flow.identity_project_id, 'hooks_project_id', flow.hooks_project_id, 'vault_project_id', flow.vault_project_id, 'input_constraints', flow.input_constraints, 'name', sink.name, 'description', sink.description) AS flow,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', node.node_id, 'node_key', node.node_key, 'node_type', node.node_type, 'parent_node_id', node.parent_node_id, 'slot', node.slot, 'slot_key', node.slot_key, 'body_type', node.body_type, 'source', node.source, 'disburse_method', node.disburse_method, 'batch_size', node.batch_size, 'max_iterations', node.max_iterations, 'prompt', node.prompt, 'parameters', node.parameters, 'transformations', node.transformations, 'schema', node.schema)) AS jsonb_agg FROM snap_nodes node
        WHERE
            node.flow_id = flow.flow_id
            AND node.version = flow.version
            AND node.is_deleted = FALSE), '[]'::jsonb) AS nodes,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', connection.connection_id, 'source_node_id', connection.source_node_id, 'parent_node_id', connection.parent_node_id, 'slot', connection.slot, 'slot_key', connection.slot_key)) AS jsonb_agg FROM snap_connections connection
        WHERE
            connection.flow_id = flow.flow_id
            AND connection.version = flow.version
            AND connection.is_deleted = FALSE), '[]'::jsonb) AS connections,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', destination.destination_id, 'connection_id', destination.connection_id, 'destination_node_id', destination.destination_node_id, 'execute_when', destination.execute_when, 'parent_node_id', destination.parent_node_id, 'slot', destination.slot, 'slot_key', destination.slot_key)) AS jsonb_agg FROM snap_connection_destinations destination
        WHERE
            destination.flow_id = flow.flow_id
            AND destination.version = flow.version
            AND destination.is_deleted = FALSE), '[]'::jsonb) AS destinations
FROM
    snap_flows flow
    LEFT JOIN sink_flows sink ON flow.flow_id = sink.flow_id;

DROP VIEW IF EXISTS flow_versions RESTRICT;

CREATE OR REPLACE VIEW flow_versions AS
SELECT
    versioned.flow_id AS id,
    versioned.version,
    (((dense_rank() OVER (PARTITION BY versioned.flow_id ORDER BY versioned.last_published NULLS FIRST) - 1)::text) || '.'::text) || ((row_number() OVER (PARTITION BY versioned.flow_id, versioned.last_published ORDER BY versioned.version) - 1)::text) AS release,
    versioned.identity_project_id,
    sink.name,
    versioned.node_count,
    versioned.connection_count,
    versioned.node_count - lag(versioned.node_count, 1, 0::bigint) OVER w AS nodes_delta,
    versioned.connection_count - lag(versioned.connection_count, 1, 0::bigint) OVER w AS connections_delta,
    versioned.actor_type,
    versioned.actor_id,
    versioned.occurred_at
FROM (
    SELECT
        flow.flow_id,
        flow.version,
        flow.identity_project_id,
        flow.actor_type,
        flow.actor_id,
        flow.occurred_at,
        flow.last_published,
        (
            SELECT
                count(*) AS count
            FROM
                snap_nodes
            WHERE
                snap_nodes.flow_id = flow.flow_id
                AND snap_nodes.version = flow.version
                AND snap_nodes.is_deleted = FALSE) AS node_count,
            (
                SELECT
                    count(*) AS count
                FROM
                    snap_connections
                WHERE
                    snap_connections.flow_id = flow.flow_id
                    AND snap_connections.version = flow.version
                    AND snap_connections.is_deleted = FALSE) AS connection_count
            FROM
                snap_flows flow
            ORDER BY
                flow.flow_id,
                flow.version) versioned
    LEFT JOIN sink_flows sink ON versioned.flow_id = sink.flow_id
WINDOW w AS (PARTITION BY versioned.flow_id ORDER BY versioned.version);

DROP VIEW IF EXISTS node_execution_io RESTRICT;

DROP VIEW IF EXISTS flow_history RESTRICT;

DROP VIEW IF EXISTS nodes RESTRICT;

DROP VIEW IF EXISTS connections RESTRICT;

DROP VIEW IF EXISTS connection_destinations RESTRICT;

DROP VIEW IF EXISTS flows RESTRICT;

CREATE OR REPLACE VIEW flows AS
SELECT
    latest_version.flow_id AS id,
    latest_version.identity_project_id,
    latest_version.hooks_project_id,
    latest_version.vault_project_id,
    sink.name,
    sink.description,
    latest_version.input_constraints,
    latest_version.version,
    latest_version.release,
    latest_version.correlation_id,
    latest_version.occurred_at AS updated_at,
    latest_version.actor_type AS updated_by_type,
    latest_version.actor_id AS updated_by_id,
    first_version.occurred_at AS created_at,
    first_version.actor_type AS created_by_type,
    first_version.actor_id AS created_by_id
FROM ( SELECT DISTINCT ON (snap_flows.flow_id)
        snap_flows.version,
        snap_flows.is_deleted,
        snap_flows.actor_type,
        snap_flows.actor_id,
        snap_flows.correlation_id,
        snap_flows.occurred_at,
        snap_flows.flow_id,
        snap_flows.identity_project_id,
        snap_flows.hooks_project_id,
        snap_flows.vault_project_id,
        snap_flows.input_constraints,
        snap_flows.last_published,
        (((dense_rank() OVER (PARTITION BY snap_flows.flow_id ORDER BY snap_flows.last_published NULLS FIRST) - 1)::text) || '.'::text) || ((row_number() OVER (PARTITION BY snap_flows.flow_id, snap_flows.last_published ORDER BY snap_flows.version) - 1)::text) AS release
FROM
    snap_flows
WHERE
    snap_flows.is_deleted = FALSE
ORDER BY
    snap_flows.flow_id,
    snap_flows.version DESC) latest_version
    JOIN ( SELECT DISTINCT ON (snap_flows.flow_id)
            snap_flows.flow_id,
            snap_flows.occurred_at,
            snap_flows.actor_type,
            snap_flows.actor_id
        FROM
            snap_flows
        ORDER BY
            snap_flows.flow_id,
            snap_flows.version) first_version ON latest_version.flow_id = first_version.flow_id
    LEFT JOIN sink_flows sink ON latest_version.flow_id = sink.flow_id;

CREATE OR REPLACE VIEW connection_destinations AS
SELECT
    latest_entity.flow_id,
    latest_entity.destination_id AS id,
    latest_entity.connection_id,
    flow.identity_project_id,
    latest_entity.destination_node_id,
    latest_entity.execute_when,
    latest_entity.parent_node_id,
    latest_entity.slot,
    latest_entity.slot_key,
    latest_entity.version,
    latest_entity.correlation_id,
    latest_entity.occurred_at AS updated_at,
    latest_entity.actor_type AS updated_by_type,
    latest_entity.actor_id AS updated_by_id,
    first_entity.occurred_at AS created_at,
    first_entity.actor_type AS created_by_type,
    first_entity.actor_id AS created_by_id
FROM
    snap_connection_destinations latest_entity
    JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    JOIN ( SELECT DISTINCT ON (snap_connection_destinations.flow_id, snap_connection_destinations.destination_id)
            snap_connection_destinations.flow_id,
            snap_connection_destinations.destination_id,
            snap_connection_destinations.occurred_at,
            snap_connection_destinations.actor_type,
            snap_connection_destinations.actor_id
        FROM
            snap_connection_destinations
        ORDER BY
            snap_connection_destinations.flow_id,
            snap_connection_destinations.destination_id,
            snap_connection_destinations.version) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.destination_id = first_entity.destination_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE OR REPLACE VIEW connections AS
SELECT
    latest_entity.flow_id,
    latest_entity.connection_id AS id,
    flow.identity_project_id,
    latest_entity.source_node_id,
    latest_entity.parent_node_id,
    latest_entity.slot,
    latest_entity.slot_key,
    latest_entity.version,
    latest_entity.correlation_id,
    latest_entity.occurred_at AS updated_at,
    latest_entity.actor_type AS updated_by_type,
    latest_entity.actor_id AS updated_by_id,
    first_entity.occurred_at AS created_at,
    first_entity.actor_type AS created_by_type,
    first_entity.actor_id AS created_by_id
FROM
    snap_connections latest_entity
    JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    JOIN ( SELECT DISTINCT ON (snap_connections.flow_id, snap_connections.connection_id)
            snap_connections.flow_id,
            snap_connections.connection_id,
            snap_connections.occurred_at,
            snap_connections.actor_type,
            snap_connections.actor_id
        FROM
            snap_connections
        ORDER BY
            snap_connections.flow_id,
            snap_connections.connection_id,
            snap_connections.version) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.connection_id = first_entity.connection_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE OR REPLACE VIEW nodes AS
SELECT
    latest_entity.flow_id,
    latest_entity.node_id AS id,
    flow.identity_project_id,
    latest_entity.node_key,
    latest_entity.node_type,
    latest_entity.parent_node_id,
    latest_entity.slot,
    latest_entity.slot_key,
    latest_entity.body_type,
    latest_entity.source,
    latest_entity.disburse_method,
    latest_entity.batch_size,
    latest_entity.max_iterations,
    latest_entity.prompt,
    latest_entity.parameters,
    latest_entity.transformations,
    latest_entity.schema,
    sink.title,
    sink.description,
    sink.icon,
    sink.color,
    sink.layout,
    latest_entity.version,
    latest_entity.correlation_id,
    latest_entity.occurred_at AS updated_at,
    latest_entity.actor_type AS updated_by_type,
    latest_entity.actor_id AS updated_by_id,
    first_entity.occurred_at AS created_at,
    first_entity.actor_type AS created_by_type,
    first_entity.actor_id AS created_by_id
FROM
    snap_nodes latest_entity
    JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    JOIN ( SELECT DISTINCT ON (snap_nodes.flow_id, snap_nodes.node_id)
            snap_nodes.flow_id,
            snap_nodes.node_id,
            snap_nodes.occurred_at,
            snap_nodes.actor_type,
            snap_nodes.actor_id
        FROM
            snap_nodes
        ORDER BY
            snap_nodes.flow_id,
            snap_nodes.node_id,
            snap_nodes.version) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.node_id = first_entity.node_id
    LEFT JOIN sink_nodes sink ON latest_entity.flow_id = sink.flow_id
        AND latest_entity.node_id = sink.node_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE OR REPLACE VIEW flow_history AS
SELECT
    flow.flow_id AS id,
    flow.version,
    (((dense_rank() OVER (PARTITION BY flow.flow_id ORDER BY flow.last_published NULLS FIRST) - 1)::text) || '.'::text) || ((row_number() OVER (PARTITION BY flow.flow_id, flow.last_published ORDER BY flow.version) - 1)::text) AS release,
    flow.identity_project_id,
    flow.occurred_at,
    jsonb_build_object('id', flow.flow_id, 'identity_project_id', flow.identity_project_id, 'hooks_project_id', flow.hooks_project_id, 'vault_project_id', flow.vault_project_id, 'input_constraints', flow.input_constraints, 'name', sink.name, 'description', sink.description) AS flow,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', node.node_id, 'node_key', node.node_key, 'node_type', node.node_type, 'parent_node_id', node.parent_node_id, 'slot', node.slot, 'slot_key', node.slot_key, 'body_type', node.body_type, 'source', node.source, 'disburse_method', node.disburse_method, 'batch_size', node.batch_size, 'max_iterations', node.max_iterations, 'prompt', node.prompt, 'parameters', node.parameters, 'transformations', node.transformations, 'schema', node.schema)) AS jsonb_agg FROM snap_nodes node
        WHERE
            node.flow_id = flow.flow_id
            AND node.version = flow.version
            AND node.is_deleted = FALSE), '[]'::jsonb) AS nodes,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', connection.connection_id, 'source_node_id', connection.source_node_id, 'parent_node_id', connection.parent_node_id, 'slot', connection.slot, 'slot_key', connection.slot_key)) AS jsonb_agg FROM snap_connections connection
        WHERE
            connection.flow_id = flow.flow_id
            AND connection.version = flow.version
            AND connection.is_deleted = FALSE), '[]'::jsonb) AS connections,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', destination.destination_id, 'connection_id', destination.connection_id, 'destination_node_id', destination.destination_node_id, 'execute_when', destination.execute_when, 'parent_node_id', destination.parent_node_id, 'slot', destination.slot, 'slot_key', destination.slot_key)) AS jsonb_agg FROM snap_connection_destinations destination
        WHERE
            destination.flow_id = flow.flow_id
            AND destination.version = flow.version
            AND destination.is_deleted = FALSE), '[]'::jsonb) AS destinations
FROM
    snap_flows flow
    LEFT JOIN sink_flows sink ON flow.flow_id = sink.flow_id;

CREATE OR REPLACE VIEW node_execution_io AS
SELECT
    latest_entity.node_execution_id AS id,
    latest_entity.identity_project_id,
    latest_entity.node_id,
    latest_entity.node_type,
    node.body_type,
    latest_entity.status,
    latest_entity.start_time,
    latest_entity.end_time,
    node.source,
    node.disburse_method,
    node.batch_size,
    node.max_iterations,
    node.prompt,
    node.parameters,
    node.transformations,
    node.schema,
    latest_entity.output,
    latest_entity.evaluation,
    latest_entity.submitted_data
FROM ( SELECT DISTINCT ON (snap_node_executions.node_execution_id)
        snap_node_executions.version,
        snap_node_executions.is_deleted,
        snap_node_executions.actor_type,
        snap_node_executions.actor_id,
        snap_node_executions.correlation_id,
        snap_node_executions.occurred_at,
        snap_node_executions.node_execution_id,
        snap_node_executions.execution_id,
        snap_node_executions.flow_id,
        snap_node_executions.identity_project_id,
        snap_node_executions.node_id,
        snap_node_executions.node_key,
        snap_node_executions.node_type,
        snap_node_executions.status,
        snap_node_executions.parent_type,
        snap_node_executions.parent_id,
        snap_node_executions.loop_index,
        snap_node_executions.directive,
        snap_node_executions.submitted_data,
        snap_node_executions.evaluation_type,
        snap_node_executions.evaluation,
        snap_node_executions.output,
        snap_node_executions.start_time,
        snap_node_executions.end_time
    FROM
        snap_node_executions
    WHERE
        snap_node_executions.is_deleted = FALSE
    ORDER BY
        snap_node_executions.node_execution_id,
        snap_node_executions.version DESC) latest_entity
    LEFT JOIN nodes node ON latest_entity.flow_id = node.flow_id
        AND latest_entity.node_id = node.id;

