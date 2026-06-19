CREATE EXTENSION IF NOT EXISTS timescaledb;

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE EXTENSION IF NOT EXISTS pg_buffercache;

CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

CREATE TYPE actor_type AS ENUM (
    'Identity',
    'System',
    'Anonymous'
);

CREATE TYPE system_actor AS ENUM (
    'FlowEngine',
    'NodeEngine',
    'Housekeeping',
    'Scheduler'
);

CREATE TYPE execution_status AS ENUM (
    'Pending',
    'Queued',
    'Running',
    'Paused',
    'AwaitingAction',
    'Completed',
    'Failed',
    'Skipped',
    'TimedOut',
    'Canceled'
);

CREATE TYPE node_type AS ENUM (
    'Action',
    'Decision',
    'WhileLoop',
    'ForeachLoop',
    'Trigger',
    'Transform',
    'Output',
    'Composite',
    'SubFlow',
    'Scope',
    'AwaitingAction'
);

CREATE TYPE disburse_method AS ENUM (
    'Sequential',
    'Batched',
    'Parallel'
);

CREATE TYPE node_body_type AS ENUM (
    'Empty',
    'ForeachLoop',
    'Decision',
    'Composite',
    'WhileLoop',
    'Group',
    'AwaitingAction'
);

CREATE TYPE node_execution_parent_type AS ENUM (
    'Root',
    'Iteration',
    'Branch'
);

CREATE TYPE node_execution_directive AS ENUM (
    'Standard',
    'Submitted'
);

CREATE TYPE node_execution_evaluation_type AS ENUM (
    'Standard',
    'ForeachLoop',
    'WhileLoop',
    'Conditional',
    'Composite'
);

CREATE TABLE IF NOT EXISTS arcv_events (
    event_id uuid NOT NULL,
    aggregate_version bigint NOT NULL,
    aggregate_id uuid NOT NULL,
    aggregate_type text NOT NULL,
    schema_version bigint NOT NULL,
    event_type text NOT NULL,
    event_data jsonb NOT NULL,
    provenance jsonb NOT NULL,
    idempotency_key text,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT arcv_events_pkey PRIMARY KEY (event_id)
);

CREATE UNIQUE INDEX index_arcv_events_aggregate_id_aggregate_version ON arcv_events (aggregate_id, aggregate_version);

CREATE INDEX index_arcv_events_aggregate_type_aggregate_id ON arcv_events (aggregate_type, aggregate_id);

CREATE TABLE IF NOT EXISTS syst_consumer_checkpoints (
    consumer_name text NOT NULL,
    last_sequence bigint NOT NULL DEFAULT 0,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT syst_consumer_checkpoints_pkey PRIMARY KEY (consumer_name)
);

CREATE TABLE IF NOT EXISTS sink_flows (
    flow_id uuid NOT NULL,
    name text NOT NULL DEFAULT '',
    description text NOT NULL DEFAULT '',
    is_deleted boolean NOT NULL DEFAULT FALSE,
    CONSTRAINT sink_flows_pkey PRIMARY KEY (flow_id)
);

CREATE INDEX index_sink_flows_name_trgm ON sink_flows USING GIN (name gin_trgm_ops);

CREATE INDEX index_sink_flows_description_trgm ON sink_flows USING GIN (description gin_trgm_ops);

CREATE TABLE IF NOT EXISTS snap_flows (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    flow_id uuid NOT NULL,
    identity_project_id text NOT NULL DEFAULT '',
    hooks_project_id text NOT NULL DEFAULT '',
    vault_project_id text NOT NULL DEFAULT '',
    input_constraints jsonb NOT NULL DEFAULT '[]',
    last_published timestamptz,
    CONSTRAINT snap_flows_pkey PRIMARY KEY (flow_id, version)
);

CREATE INDEX index_snap_flows_latest ON snap_flows (flow_id, version DESC)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_flows_identity_project_id ON snap_flows (identity_project_id);

CREATE INDEX index_snap_flows_correlation_id ON snap_flows (correlation_id);

CREATE INDEX index_snap_flows_edition ON snap_flows (flow_id, last_published, version);

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
FROM ( SELECT DISTINCT ON (flow_id)
        *,
        (DENSE_RANK() OVER (PARTITION BY flow_id ORDER BY last_published NULLS FIRST) - 1)::text || '.' || (ROW_NUMBER() OVER (PARTITION BY flow_id, last_published ORDER BY version) - 1)::text AS release
FROM
    snap_flows
WHERE
    is_deleted = FALSE
ORDER BY
    flow_id,
    version DESC) latest_version
    INNER JOIN ( SELECT DISTINCT ON (flow_id)
            flow_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_flows
        ORDER BY
            flow_id,
            version ASC) first_version ON latest_version.flow_id = first_version.flow_id
    LEFT JOIN sink_flows sink ON latest_version.flow_id = sink.flow_id;

CREATE OR REPLACE VIEW deleted_flows AS SELECT DISTINCT ON (snap.flow_id)
    snap.flow_id AS id,
    sink.name,
    snap.identity_project_id,
    sink.description,
    snap.actor_type AS deleted_by_type,
    snap.actor_id AS deleted_by_id,
    snap.occurred_at AS deleted_at
FROM
    snap_flows snap
    LEFT JOIN sink_flows sink ON snap.flow_id = sink.flow_id
WHERE
    snap.is_deleted = TRUE
ORDER BY
    snap.flow_id,
    snap.version DESC;

CREATE TABLE IF NOT EXISTS snap_executions (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    execution_id uuid NOT NULL,
    flow_id uuid NOT NULL,
    flow_version bigint NOT NULL,
    identity_project_id text NOT NULL DEFAULT '',
    status execution_status NOT NULL,
    input jsonb,
    output jsonb,
    start_time timestamptz,
    end_time timestamptz,
    paused_at timestamptz,
    resumed_at timestamptz,
    CONSTRAINT snap_executions_pkey PRIMARY KEY (execution_id, version)
);

CREATE INDEX index_snap_executions_latest ON snap_executions (execution_id, version DESC)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_executions_flow_id ON snap_executions (flow_id);

CREATE INDEX index_snap_executions_flow_id_status ON snap_executions (flow_id, status);

CREATE INDEX index_snap_executions_identity_project_id_status ON snap_executions (identity_project_id, status);

CREATE INDEX index_snap_executions_identity_project_id_occurred_at ON snap_executions (identity_project_id, occurred_at);

CREATE INDEX index_snap_executions_correlation_id ON snap_executions (correlation_id);

CREATE INDEX index_snap_executions_end_time ON snap_executions (end_time);

CREATE OR REPLACE VIEW executions AS
SELECT
    latest_version.execution_id AS id,
    latest_version.flow_id,
    latest_version.flow_version,
    latest_version.identity_project_id,
    latest_version.status,
    latest_version.input,
    latest_version.output,
    latest_version.start_time,
    latest_version.end_time,
    latest_version.paused_at,
    latest_version.resumed_at,
    latest_version.version,
    latest_version.correlation_id,
    latest_version.occurred_at AS updated_at,
    latest_version.actor_type AS updated_by_type,
    latest_version.actor_id AS updated_by_id,
    first_version.occurred_at AS created_at,
    first_version.actor_type AS created_by_type,
    first_version.actor_id AS created_by_id
FROM ( SELECT DISTINCT ON (execution_id)
        *
    FROM
        snap_executions
    WHERE
        is_deleted = FALSE
    ORDER BY
        execution_id,
        version DESC) latest_version
    INNER JOIN ( SELECT DISTINCT ON (execution_id)
            execution_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_executions
        ORDER BY
            execution_id,
            version ASC) first_version ON latest_version.execution_id = first_version.execution_id;

CREATE OR REPLACE VIEW deleted_executions AS SELECT DISTINCT ON (execution_id)
    snap.execution_id AS id,
    snap.flow_id,
    snap.flow_version,
    snap.identity_project_id,
    snap.status,
    snap.actor_type AS deleted_by_type,
    snap.actor_id AS deleted_by_id,
    snap.occurred_at AS deleted_at
FROM
    snap_executions snap
WHERE
    is_deleted = TRUE
ORDER BY
    execution_id,
    version DESC;

CREATE TABLE IF NOT EXISTS snap_connections (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    flow_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    source_node_id uuid NOT NULL,
    parent_node_id uuid,
    slot text NOT NULL DEFAULT 'root',
    slot_key text,
    CONSTRAINT snap_connections_pkey PRIMARY KEY (flow_id, version, connection_id)
);

CREATE INDEX index_snap_connections_flow_version ON snap_connections (flow_id, version)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_connections_flow_id_source_node_id ON snap_connections (flow_id, source_node_id);

CREATE INDEX index_snap_connections_correlation_id ON snap_connections (correlation_id);

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
    INNER JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    INNER JOIN ( SELECT DISTINCT ON (flow_id, connection_id)
            flow_id,
            connection_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_connections
        ORDER BY
            flow_id,
            connection_id,
            version ASC) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.connection_id = first_entity.connection_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE TABLE IF NOT EXISTS snap_connection_destinations (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    flow_id uuid NOT NULL,
    destination_id uuid NOT NULL,
    connection_id uuid NOT NULL,
    destination_node_id uuid NOT NULL,
    execute_when text[] NOT NULL DEFAULT '{}',
    parent_node_id uuid,
    slot text NOT NULL DEFAULT 'root',
    slot_key text,
    CONSTRAINT snap_connection_destinations_pkey PRIMARY KEY (flow_id, version, connection_id, destination_id)
);

CREATE INDEX index_snap_connection_destinations_flow_version ON snap_connection_destinations (flow_id, version)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_connection_destinations_flow_id_connection_id ON snap_connection_destinations (flow_id, connection_id);

CREATE INDEX index_snap_connection_destinations_flow_id_destination_node_id ON snap_connection_destinations (flow_id, destination_node_id);

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
    INNER JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    INNER JOIN ( SELECT DISTINCT ON (flow_id, destination_id)
            flow_id,
            destination_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_connection_destinations
        ORDER BY
            flow_id,
            destination_id,
            version ASC) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.destination_id = first_entity.destination_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE TABLE IF NOT EXISTS sink_nodes (
    flow_id uuid NOT NULL,
    node_id uuid NOT NULL,
    title text NOT NULL DEFAULT '',
    description text NOT NULL DEFAULT '',
    icon text NOT NULL DEFAULT '',
    color text NOT NULL DEFAULT '',
    layout jsonb,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    CONSTRAINT sink_nodes_pkey PRIMARY KEY (flow_id, node_id)
);

CREATE INDEX index_sink_nodes_title_trgm ON sink_nodes USING GIN (title gin_trgm_ops);

CREATE INDEX index_sink_nodes_description_trgm ON sink_nodes USING GIN (description gin_trgm_ops);

CREATE TABLE IF NOT EXISTS snap_nodes (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    flow_id uuid NOT NULL,
    node_id uuid NOT NULL,
    node_key text NOT NULL,
    node_type node_type NOT NULL,
    parent_node_id uuid,
    slot text NOT NULL DEFAULT 'root',
    slot_key text,
    body_type node_body_type NOT NULL DEFAULT 'Empty',
    source text,
    disburse_method disburse_method,
    batch_size bigint,
    max_iterations integer,
    prompt text,
    parameters jsonb,
    transformations jsonb,
    schema jsonb,
    CONSTRAINT snap_nodes_pkey PRIMARY KEY (flow_id, version, node_id)
);

CREATE INDEX index_snap_nodes_flow_version ON snap_nodes (flow_id, version)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_nodes_node_type ON snap_nodes (node_type);

CREATE INDEX index_snap_nodes_flow_id_node_key ON snap_nodes (flow_id, node_key);

CREATE INDEX index_snap_nodes_correlation_id ON snap_nodes (correlation_id);

CREATE INDEX index_snap_nodes_parent_node_id ON snap_nodes (parent_node_id)
WHERE
    parent_node_id IS NOT NULL;

CREATE INDEX index_snap_nodes_node_key_trgm ON snap_nodes USING GIN (node_key gin_trgm_ops);

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
    INNER JOIN flows flow ON latest_entity.flow_id = flow.id
        AND latest_entity.version = flow.version
    INNER JOIN ( SELECT DISTINCT ON (flow_id, node_id)
            flow_id,
            node_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_nodes
        ORDER BY
            flow_id,
            node_id,
            version ASC) first_entity ON latest_entity.flow_id = first_entity.flow_id
    AND latest_entity.node_id = first_entity.node_id
    LEFT JOIN sink_nodes sink ON latest_entity.flow_id = sink.flow_id
        AND latest_entity.node_id = sink.node_id
WHERE
    latest_entity.is_deleted = FALSE;

CREATE TABLE IF NOT EXISTS snap_node_executions (
    version bigint NOT NULL,
    is_deleted boolean NOT NULL DEFAULT FALSE,
    actor_type actor_type NOT NULL DEFAULT 'Anonymous'::actor_type,
    actor_id text NOT NULL DEFAULT '',
    correlation_id uuid NOT NULL,
    occurred_at timestamptz NOT NULL,
    node_execution_id uuid NOT NULL,
    execution_id uuid NOT NULL,
    flow_id uuid NOT NULL,
    identity_project_id text NOT NULL DEFAULT '',
    node_id uuid NOT NULL,
    node_key text NOT NULL,
    node_type node_type NOT NULL,
    status execution_status NOT NULL,
    parent_type node_execution_parent_type NOT NULL DEFAULT 'Root'::node_execution_parent_type,
    parent_id uuid,
    loop_index bigint,
    directive node_execution_directive NOT NULL DEFAULT 'Standard'::node_execution_directive,
    submitted_data jsonb,
    evaluation_type node_execution_evaluation_type NOT NULL DEFAULT 'Standard'::node_execution_evaluation_type,
    evaluation jsonb,
    output jsonb,
    start_time timestamptz,
    end_time timestamptz,
    CONSTRAINT snap_node_executions_pkey PRIMARY KEY (node_execution_id, version)
);

CREATE INDEX index_snap_node_executions_latest ON snap_node_executions (node_execution_id, version DESC)
WHERE
    is_deleted = FALSE;

CREATE INDEX index_snap_node_executions_execution_id ON snap_node_executions (execution_id);

CREATE INDEX index_snap_node_executions_execution_id_node_key ON snap_node_executions (execution_id, node_key);

CREATE INDEX index_snap_node_executions_status ON snap_node_executions (status);

CREATE INDEX index_snap_node_executions_identity_project_id_status ON snap_node_executions (identity_project_id, status);

CREATE INDEX index_snap_node_executions_flow_id_node_type ON snap_node_executions (flow_id, node_type);

CREATE INDEX index_snap_node_executions_correlation_id ON snap_node_executions (correlation_id);

CREATE INDEX index_snap_node_executions_node_id ON snap_node_executions (node_id);

CREATE INDEX index_snap_node_executions_parent_id ON snap_node_executions (parent_id);

CREATE OR REPLACE VIEW node_executions AS
SELECT
    latest_version.node_execution_id AS id,
    latest_version.execution_id,
    latest_version.flow_id,
    latest_version.identity_project_id,
    latest_version.node_id,
    latest_version.node_key,
    latest_version.node_type,
    latest_version.status,
    latest_version.parent_type,
    latest_version.parent_id,
    latest_version.loop_index,
    latest_version.directive,
    latest_version.evaluation_type,
    latest_version.start_time,
    latest_version.end_time,
    latest_version.version,
    latest_version.correlation_id,
    latest_version.occurred_at AS updated_at,
    latest_version.actor_type AS updated_by_type,
    latest_version.actor_id AS updated_by_id,
    first_version.occurred_at AS created_at,
    first_version.actor_type AS created_by_type,
    first_version.actor_id AS created_by_id
FROM ( SELECT DISTINCT ON (node_execution_id)
        *
    FROM
        snap_node_executions
    WHERE
        is_deleted = FALSE
    ORDER BY
        node_execution_id,
        version DESC) latest_version
    INNER JOIN ( SELECT DISTINCT ON (node_execution_id)
            node_execution_id,
            occurred_at,
            actor_type,
            actor_id
        FROM
            snap_node_executions
        ORDER BY
            node_execution_id,
            version ASC) first_version ON latest_version.node_execution_id = first_version.node_execution_id;

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
FROM ( SELECT DISTINCT ON (node_execution_id)
        *
    FROM
        snap_node_executions
    WHERE
        is_deleted = FALSE
    ORDER BY
        node_execution_id,
        version DESC) latest_entity
    LEFT JOIN nodes node ON latest_entity.flow_id = node.flow_id
        AND latest_entity.node_id = node.id;

CREATE OR REPLACE VIEW deleted_node_executions AS SELECT DISTINCT ON (node_execution_id)
    snap.node_execution_id AS id,
    snap.execution_id,
    snap.flow_id,
    snap.identity_project_id,
    snap.status,
    snap.actor_type AS deleted_by_type,
    snap.actor_id AS deleted_by_id,
    snap.occurred_at AS deleted_at
FROM
    snap_node_executions snap
WHERE
    is_deleted = TRUE
ORDER BY
    node_execution_id,
    version DESC;

CREATE OR REPLACE VIEW flow_versions AS
SELECT
    versioned.flow_id AS id,
    versioned.version,
    (DENSE_RANK() OVER (PARTITION BY versioned.flow_id ORDER BY versioned.last_published NULLS FIRST) - 1)::text || '.' || (ROW_NUMBER() OVER (PARTITION BY versioned.flow_id, versioned.last_published ORDER BY versioned.version) - 1)::text AS release,
    versioned.identity_project_id,
    sink.name,
    versioned.node_count,
    versioned.connection_count,
    versioned.node_count - LAG(versioned.node_count, 1, 0::bigint) OVER w AS nodes_delta,
    versioned.connection_count - LAG(versioned.connection_count, 1, 0::bigint) OVER w AS connections_delta,
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
                COUNT(*)
            FROM
                snap_nodes
            WHERE
                flow_id = flow.flow_id
                AND version = flow.version
                AND is_deleted = FALSE) AS node_count,
            (
                SELECT
                    COUNT(*)
                FROM
                    snap_connections
                WHERE
                    flow_id = flow.flow_id
                    AND version = flow.version
                    AND is_deleted = FALSE) AS connection_count
            FROM
                snap_flows flow
            ORDER BY
                flow.flow_id,
                flow.version) versioned
    LEFT JOIN sink_flows sink ON versioned.flow_id = sink.flow_id
WINDOW w AS (PARTITION BY versioned.flow_id ORDER BY versioned.version);

CREATE OR REPLACE VIEW flow_history AS
SELECT
    flow.flow_id AS id,
    flow.version,
    (DENSE_RANK() OVER (PARTITION BY flow.flow_id ORDER BY flow.last_published NULLS FIRST) - 1)::text || '.' || (ROW_NUMBER() OVER (PARTITION BY flow.flow_id, flow.last_published ORDER BY flow.version) - 1)::text AS release,
    flow.identity_project_id,
    flow.occurred_at,
    jsonb_build_object('id', flow.flow_id, 'identity_project_id', flow.identity_project_id, 'hooks_project_id', flow.hooks_project_id, 'vault_project_id', flow.vault_project_id, 'input_constraints', flow.input_constraints, 'name', sink.name, 'description', sink.description) AS flow,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', node.node_id, 'node_key', node.node_key, 'node_type', node.node_type, 'parent_node_id', node.parent_node_id, 'slot', node.slot, 'slot_key', node.slot_key, 'body_type', node.body_type, 'source', node.source, 'disburse_method', node.disburse_method, 'batch_size', node.batch_size, 'max_iterations', node.max_iterations, 'prompt', node.prompt, 'parameters', node.parameters, 'transformations', node.transformations, 'schema', node.schema))
        FROM snap_nodes node
        WHERE
            node.flow_id = flow.flow_id
            AND node.version = flow.version
            AND node.is_deleted = FALSE), '[]'::jsonb) AS nodes,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', connection.connection_id, 'source_node_id', connection.source_node_id, 'parent_node_id', connection.parent_node_id, 'slot', connection.slot, 'slot_key', connection.slot_key))
        FROM snap_connections connection
        WHERE
            connection.flow_id = flow.flow_id
            AND connection.version = flow.version
            AND connection.is_deleted = FALSE), '[]'::jsonb) AS connections,
    COALESCE((
        SELECT
            jsonb_agg(jsonb_build_object('id', destination.destination_id, 'connection_id', destination.connection_id, 'destination_node_id', destination.destination_node_id, 'execute_when', destination.execute_when, 'parent_node_id', destination.parent_node_id, 'slot', destination.slot, 'slot_key', destination.slot_key))
        FROM snap_connection_destinations destination
        WHERE
            destination.flow_id = flow.flow_id
            AND destination.version = flow.version
            AND destination.is_deleted = FALSE), '[]'::jsonb) AS destinations
FROM
    snap_flows flow
    LEFT JOIN sink_flows sink ON flow.flow_id = sink.flow_id;

