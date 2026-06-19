-- Raw SQL appended after the YAML-generated schema.
-- Use this file for statements pgschema tracks but can't be expressed via
-- YAML/template rendering — typically privilege changes on extension-owned
-- objects that exist in the target database.
--
-- Anything here must be idempotent and describe state that already holds in
-- the live database — the postlude aligns pgschema's desired state with
-- reality, it does NOT mutate.
-- ─────────────────────────────────────────────────────────────────────────
-- pg_buffercache extension
-- ─────────────────────────────────────────────────────────────────────────
-- The extension revokes PUBLIC's default EXECUTE and grants it to pg_monitor
-- instead so only monitoring roles can inspect the buffer cache.
-- Schema-qualify with `public.` so statements resolve regardless of the
-- search_path in pgschema's temporary plan schema.
REVOKE EXECUTE ON FUNCTION public.pg_buffercache_pages()
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.pg_buffercache_pages() TO pg_monitor;
REVOKE EXECUTE ON FUNCTION public.pg_buffercache_summary(
    OUT buffers_used integer,
    OUT buffers_unused integer,
    OUT buffers_dirty integer,
    OUT buffers_pinned integer,
    OUT usagecount_avg double precision
)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.pg_buffercache_summary(
        OUT buffers_used integer,
        OUT buffers_unused integer,
        OUT buffers_dirty integer,
        OUT buffers_pinned integer,
        OUT usagecount_avg double precision
    ) TO pg_monitor;
REVOKE EXECUTE ON FUNCTION public.pg_buffercache_usage_counts(
    OUT usage_count integer,
    OUT buffers integer,
    OUT dirty integer,
    OUT pinned integer
)
FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.pg_buffercache_usage_counts(
        OUT usage_count integer,
        OUT buffers integer,
        OUT dirty integer,
        OUT pinned integer
    ) TO pg_monitor;
REVOKE
SELECT ON TABLE public.pg_buffercache
FROM PUBLIC;
GRANT SELECT ON TABLE public.pg_buffercache TO pg_monitor;
-- ─────────────────────────────────────────────────────────────────────────
-- pg_stat_statements extension
-- ─────────────────────────────────────────────────────────────────────────
-- The extension revokes PUBLIC's default access on the stats views and on
-- pg_stat_statements_reset(). No role is explicitly granted in its place;
-- superuser / pg_read_all_stats membership governs access.
REVOKE
SELECT ON TABLE public.pg_stat_statements
FROM PUBLIC;
REVOKE
SELECT ON TABLE public.pg_stat_statements_info
FROM PUBLIC;
REVOKE EXECUTE ON FUNCTION public.pg_stat_statements_reset(
    userid oid,
    dbid oid,
    queryid bigint,
    minmax_only boolean
)
FROM PUBLIC;