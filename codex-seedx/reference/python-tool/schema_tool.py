#!/usr/bin/env python3
"""Schema tool: validate YAML, generate SQL, run migrations.

Usage:
    schema_tool.py validate              Validate YAML against schema
    schema_tool.py generate              Generate schema.sql
    schema_tool.py migrate               Apply pending migrations
    schema_tool.py revision <name>       Create migration from DB diff
    schema_tool.py current               Show latest applied migration
    schema_tool.py history               List all applied migrations
"""
import os
import re
import sys
import json
import glob
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path

import yaml
from jsonschema import Draft202012Validator
from jinja2 import Environment, FileSystemLoader


def resolve_project_root() -> Path:
    if configured := os.environ.get("LAX_PROJECT_ROOT"):
        return Path(configured)
    script_parent = Path(__file__).resolve().parent
    # Repo layout: <repo>/tooling/docker/migrator/schema_tool.py — walk three
    # parents up to land on the repo root.
    if (
        script_parent.name == "migrator"
        and script_parent.parent.name == "docker"
        and script_parent.parent.parent.name == "tooling"
    ):
        return script_parent.parent.parent.parent
    raise RuntimeError(
        "Set LAX_PROJECT_ROOT to the repository root, or run this file from "
        "tooling/docker/migrator/ inside the repo checkout."
    )


PROJECT_ROOT = resolve_project_root()
SCHEMA_DIR = PROJECT_ROOT / "infrastructure" / "persistence" / "src" / "schema"
DEFINITIONS_DIR = SCHEMA_DIR / "definitions"
TEMPLATES_DIR = SCHEMA_DIR / "templates"
GENERATED_DIR = SCHEMA_DIR / "generated"
SCHEMA_PATH = SCHEMA_DIR / "aggregate-schema.json"
MIGRATIONS_DIR = Path(os.environ.get("MIGRATIONS_DIR", PROJECT_ROOT / "migrations"))

GREEN = "\033[32m"
RED = "\033[31m"
CYAN = "\033[36m"
DIM = "\033[2m"
BOLD = "\033[1m"
RESET = "\033[0m"

YELLOW = "\033[33m"
NO_COLOR = not sys.stdout.isatty() or os.environ.get("NO_COLOR")
if NO_COLOR:
    GREEN = RED = YELLOW = CYAN = DIM = BOLD = RESET = ""


def pg_format_sql(sql):
    """Pipe SQL through pg_format if available; return unchanged on failure."""
    try:
        result = subprocess.run(
            ["pg_format"], input=sql, capture_output=True, text=True
        )
        if result.returncode == 0:
            return result.stdout
    except FileNotFoundError:
        pass
    return sql


def ok(message):
    print(f"  {GREEN}✓{RESET} {message}")


def fail(message):
    print(f"  {RED}✗{RESET} {message}")


def info(message):
    print(f"  {DIM}{message}{RESET}")


def header(title):
    print(f"\n{BOLD}{title}{RESET}")


def human_time(timestamp_str):
    try:
        timestamp = datetime.fromisoformat(timestamp_str.strip())
        now = datetime.now(timezone.utc)
        delta = now - timestamp
        seconds = int(delta.total_seconds())
        if seconds < 60:
            return "just now"
        if seconds < 3600:
            minutes = seconds // 60
            return f"{minutes}m ago"
        if seconds < 86400:
            hours = seconds // 3600
            return f"{hours}h ago"
        days = seconds // 86400
        if days < 30:
            return f"{days}d ago"
        return timestamp.strftime("%b %d, %Y")
    except (ValueError, TypeError):
        return timestamp_str.strip()


def db_url():
    return os.environ.get(
        "DATABASE_URL", "postgres://postgres:postgres@127.0.0.1:5432/core"
    )


def db_env():
    return {
        "PGHOST": os.environ.get("PGHOST", "127.0.0.1"),
        "PGPORT": os.environ.get("PGPORT", "5432"),
        "PGUSER": os.environ.get("PGUSER", "postgres"),
        "PGPASSWORD": os.environ.get("PGPASSWORD", "postgres"),
        "PGDATABASE": os.environ.get("PGDATABASE", "core"),
    }


def psql(url, command, capture=False):
    result = subprocess.run(
        ["psql", url, "-tAc", command],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0 and not capture:
        fail(f"psql: {result.stderr.strip()}")
        sys.exit(1)
    return result.stdout.strip() if capture else result.returncode


def psql_file(url, filepath):
    result = subprocess.run(
        ["psql", url, "-f", str(filepath)], capture_output=True, text=True
    )
    if result.returncode != 0:
        fail(f"psql: {result.stderr.strip()}")
    return result.returncode


def wait_for_db(url, retries=30):
    for attempt in range(retries):
        result = subprocess.run(
            ["psql", url, "-tAc", "SELECT 1"], capture_output=True, text=True
        )
        if result.stdout.strip() == "1":
            return
        if attempt == retries - 1:
            fail("database not reachable after 30 attempts")
            sys.exit(1)
        time.sleep(1)


def load_schema():
    with open(SCHEMA_PATH) as file:
        return json.load(file)


def discover_definition_files():
    """Return all definition files in a deterministic order.

    Order: `shared.yml` → `<aggregate>/root.yml` → alphabetical entity files
    → `<aggregate>/history.yml` (cross-entity composition views).

    `shared.yml` defines types and extensions referenced by every aggregate.
    `<aggregate>/root.yml` creates the aggregate's root table + main view so
    entity views can join against it. `<aggregate>/history.yml` is loaded last
    because it composes snapshots from every sibling entity and must see all
    their tables already created.
    """
    all_files = sorted(DEFINITIONS_DIR.rglob("*.yml"))
    shared = [
        path
        for path in all_files
        if path.parent == DEFINITIONS_DIR and path.stem == "shared"
    ]
    roots = [path for path in all_files if path.stem == "root" and path not in shared]
    histories = [
        path for path in all_files if path.stem == "history" and path not in shared
    ]
    others = [
        path
        for path in all_files
        if path not in shared and path not in roots and path not in histories
    ]
    return shared + roots + others + histories


def definition_label(path):
    return str(path.relative_to(DEFINITIONS_DIR))


def load_definition(path):
    with open(path) as file:
        return yaml.safe_load(file) or {}


def validate_definition(schema, data, label):
    validator = Draft202012Validator(schema)
    errors = list(validator.iter_errors(data))
    if errors:
        fail(f"{label} — {len(errors)} errors")
        for error in errors[:5]:
            print(f"      {error.message[:120]}")
        return False
    return True


def merge_partials(definitions):
    """Collect `x_partials` from every file into one shared map.

    Any file can reference any partial after merging. Partials are expected to
    be named uniquely across files; a later file redefining a partial replaces
    the earlier value.
    """
    merged = {}
    for _, data in definitions:
        for name, body in (data.get("x_partials") or {}).items():
            merged[name] = body
    return merged


def render_sql(template, data):
    sql = template.render(**data)
    sql = re.sub(r"\n{2,}", "\n", sql).strip() + "\n"
    sql = re.sub(r"\n(CREATE )", r"\n\n\1", sql)
    return sql


def summary(data):
    parts = []
    for key in ["extensions", "types", "tables", "sequences", "indexes", "views"]:
        count = len(data.get(key, []))
        if count:
            parts.append(f"{count} {key}")
    return ", ".join(parts)


def cmd_validate():
    header("Validating definitions")
    schema = load_schema()
    all_valid = True
    for path in discover_definition_files():
        label = definition_label(path)
        data = load_definition(path)
        if validate_definition(schema, data, label):
            ok(f"{label} {DIM}({summary(data)}){RESET}")
        else:
            all_valid = False
    if not all_valid:
        sys.exit(1)


def cmd_generate():
    header("Generating SQL")
    schema = load_schema()
    env = Environment(
        loader=FileSystemLoader(str(TEMPLATES_DIR)), keep_trailing_newline=True
    )
    template = env.get_template("schema.sql.jinja")

    definitions = []
    for path in discover_definition_files():
        label = definition_label(path)
        data = load_definition(path)
        if not validate_definition(schema, data, label):
            sys.exit(1)
        definitions.append((label, data))

    shared_partials = merge_partials(definitions)

    all_sql = []
    total_statements = 0
    for label, data in definitions:
        render_context = dict(data)
        render_context["x_partials"] = {
            **shared_partials,
            **(data.get("x_partials") or {}),
        }
        sql = render_sql(template, render_context)
        statement_count = sql.count(";")
        total_statements += statement_count
        ok(f"{label} {DIM}→ {statement_count} statements{RESET}")
        all_sql.append(sql)

    GENERATED_DIR.mkdir(parents=True, exist_ok=True)
    combined = pg_format_sql("\n".join(all_sql))
    (GENERATED_DIR / "schema.sql").write_text(combined)
    line_count = combined.count("\n") + 1
    info(f"schema.sql → {total_statements} statements, {line_count} lines")


def cmd_migrate():
    header("Applying migrations")
    url = db_url()
    wait_for_db(url)

    psql(
        url,
        """
        CREATE TABLE IF NOT EXISTS _migrations (
            filename    text PRIMARY KEY,
            applied_at  timestamptz NOT NULL DEFAULT now()
        );
    """,
    )

    applied = 0
    skipped = 0
    for filepath in sorted(glob.glob(str(MIGRATIONS_DIR / "*.sql"))):
        filename = Path(filepath).name
        applied_at = psql(
            url,
            f"SELECT applied_at FROM _migrations WHERE filename = '{filename}';",
            capture=True,
        )
        if applied_at:
            info(f"{filename} {DIM}— {human_time(applied_at)}{RESET}")
            skipped += 1
            continue

        if psql_file(url, filepath) != 0:
            sys.exit(1)
        psql(url, f"INSERT INTO _migrations (filename) VALUES ('{filename}');")
        ok(f"{CYAN}{filename}{RESET} {DIM}— just applied{RESET}")
        applied += 1

    print()
    info(f"{applied} new, {skipped} already applied")


def cmd_current():
    header("Current migration")
    url = db_url()
    result = psql(
        url,
        "SELECT filename, applied_at FROM _migrations ORDER BY filename DESC LIMIT 1;",
        capture=True,
    )
    if result:
        parts = result.split("|")
        ok(f"{CYAN}{parts[0].strip()}{RESET} {DIM}— {human_time(parts[1])}{RESET}")
    else:
        info("no migrations applied")


def cmd_history():
    header("Migration history")
    url = db_url()
    result = psql(
        url,
        "SELECT filename, applied_at FROM _migrations ORDER BY filename;",
        capture=True,
    )
    if not result:
        info("no migrations applied")
        return
    for line in result.strip().split("\n"):
        parts = line.split("|")
        ok(f"{parts[0].strip()} {DIM}— {human_time(parts[1])}{RESET}")


def cmd_revision(name):
    cmd_generate()

    header("Creating revision")
    env_vars = db_env()
    schema_sql = str(GENERATED_DIR / "schema.sql")
    diff_path = "/tmp/_pgschema_diff.sql"

    result = subprocess.run(
        [
            "pgschema",
            "plan",
            "--host",
            env_vars["PGHOST"],
            "--db",
            env_vars["PGDATABASE"],
            "--user",
            env_vars["PGUSER"],
            "--password",
            env_vars["PGPASSWORD"],
            "--plan-host",
            env_vars["PGHOST"],
            "--plan-db",
            env_vars["PGDATABASE"],
            "--plan-user",
            env_vars["PGUSER"],
            "--plan-password",
            env_vars["PGPASSWORD"],
            "--file",
            schema_sql,
            "--output-sql",
            diff_path,
        ]
    )

    if result.returncode != 0:
        sys.exit(result.returncode)

    diff_content = Path(diff_path).read_text() if Path(diff_path).exists() else ""
    if not diff_content.strip():
        info("no schema changes detected")
        return

    diff_content = re.sub(r" CONCURRENTLY", "", diff_content)
    diff_content = re.sub(r"-- pgschema:wait.*?\n\n", "", diff_content, flags=re.DOTALL)
    diff_content = re.sub(r"-- Transaction Group.*?\n", "", diff_content)

    # Strip spurious GRANT/REVOKE statements targeting Postgres extension-owned
    # functions/tables. pgschema flags these as drift because its temp-schema
    # comparison can't see extension privileges, but applying them would just
    # toggle Postgres defaults on extension objects we don't own.
    diff_content = re.sub(
        r"^(?:GRANT|REVOKE)[^;]*pg_(?:buffercache|stat_statements)[^;]*;\n?",
        "",
        diff_content,
        flags=re.MULTILINE,
    )

    timestamp = datetime.now().strftime("%Y%m%d%H%M%S")
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    migration_file = MIGRATIONS_DIR / f"{timestamp}_{name}.sql"
    MIGRATIONS_DIR.mkdir(parents=True, exist_ok=True)

    content = (
        f"-- ============================================================================\n"
        f"-- Migration: {name}\n"
        f"-- Generated: {generated_at}\n"
        f"-- ============================================================================\n"
        f"\n{diff_content.strip()}\n\n"
        f"-- ============================================================================\n"
        f"-- CUSTOM DATA MIGRATION — add manual SQL below this line\n"
        f"-- ============================================================================\n"
    )

    try:
        result = subprocess.run(
            ["pg_format"], input=content, capture_output=True, text=True
        )
        if result.returncode == 0:
            content = result.stdout
    except FileNotFoundError:
        pass

    migration_file.write_text(content)
    ok(f"created {CYAN}{migration_file.relative_to(PROJECT_ROOT)}{RESET}")


EXTENSION_NOISE_PATTERN = re.compile(r"pg_(?:buffercache|stat_statements)")


def _colorize_diff_markers(text):
    """Re-color the `+` / `-` / `~` diff markers pgschema would have colored
    if we hadn't passed `--no-color`. No-op when colors are disabled."""
    if not GREEN:
        return text
    text = re.sub(r"(?m)^(\s+)\+", f"\\1{GREEN}+{RESET}", text)
    text = re.sub(r"(?m)^(\s+)-", f"\\1{RED}-{RESET}", text)
    text = re.sub(r"(?m)^(\s+)~", f"\\1{YELLOW}~{RESET}", text)
    return text


def _filter_plan_output(text):
    """Strip extension-owned privilege noise from pgschema's human output.

    pgschema flags REVOKE/GRANT on Postgres extension functions (pg_buffercache,
    pg_stat_statements) as drift because its temp-schema comparison can't see
    them, but `cmd_revision` strips these from the generated migration. This
    function keeps `db-check` output consistent with the actual migration.
    """
    lines = text.splitlines(keepends=True)
    filtered = []
    noise_count = 0
    in_revoked_block = False
    for line in lines:
        if line.rstrip() == "Revoked default privileges:":
            in_revoked_block = True
            filtered.append(line)
            continue
        if in_revoked_block and line.lstrip().startswith("-"):
            if EXTENSION_NOISE_PATTERN.search(line):
                noise_count += 1
                continue
            filtered.append(line)
            continue
        if in_revoked_block and not line.lstrip().startswith("-"):
            in_revoked_block = False
        filtered.append(line)
    text = "".join(filtered)

    if noise_count == 0:
        return text

    # Adjust the `Plan: N to add, M to drop, K to modify.` header (any subset
    # of the three clauses may appear). All noise items are drops, so shave
    # noise_count off the drop tally and rebuild the header. If nothing is
    # left, collapse to a `No schema changes detected.` line.
    def _sub_total(match):
        parts = re.findall(r"(\d+) to (\w+)", match.group(1))
        adjusted = []
        for count_str, action in parts:
            count = int(count_str)
            if action == "drop":
                count = max(0, count - noise_count)
            if count > 0:
                adjusted.append(f"{count} to {action}")
        return (
            f"Plan: {', '.join(adjusted)}."
            if adjusted
            else "No schema changes detected."
        )

    text = re.sub(r"Plan: ([^.]+)\.", _sub_total, text, count=1)

    def _sub_summary(match):
        total = int(match.group(1)) - noise_count
        return "" if total <= 0 else f"  revoked default privileges: {total} to drop\n"

    text = re.sub(r"  revoked default privileges: (\d+) to drop\n", _sub_summary, text)

    text = re.sub(r"Revoked default privileges:\n(?=\n|\Z)", "", text)

    # Drop the `Summary by type:` header if it has no body left after filtering.
    text = re.sub(r"\nSummary by type:\n(?=\n|\Z)", "", text)

    # Strip matching GRANT/REVOKE statements from the "DDL to be executed"
    # section so the preview matches what the generated migration will contain.
    text = re.sub(
        r"^(?:GRANT|REVOKE)[^;]*pg_(?:buffercache|stat_statements)[^;]*;\n?",
        "",
        text,
        flags=re.MULTILINE,
    )
    return text


def cmd_check():
    cmd_generate()

    header("Checking plan")
    env_vars = db_env()
    schema_sql = str(GENERATED_DIR / "schema.sql")
    plan_sql = GENERATED_DIR / "plan.sql"

    result = subprocess.run(
        [
            "pgschema",
            "plan",
            "--host",
            env_vars["PGHOST"],
            "--db",
            env_vars["PGDATABASE"],
            "--user",
            env_vars["PGUSER"],
            "--password",
            env_vars["PGPASSWORD"],
            "--plan-host",
            env_vars["PGHOST"],
            "--plan-db",
            env_vars["PGDATABASE"],
            "--plan-user",
            env_vars["PGUSER"],
            "--plan-password",
            env_vars["PGPASSWORD"],
            "--file",
            schema_sql,
            "--no-color",
            "--output-human",
            "stdout",
            "--output-sql",
            str(plan_sql),
        ],
        capture_output=True,
        text=True,
    )

    # Strip the inline DDL dump from stdout — it's redundant now that the
    # full plan lives in `plan.sql`. Keep the summary + per-type listings,
    # then re-apply diff-marker colors that pgschema emitted before `--no-color`.
    filtered = _filter_plan_output(result.stdout)
    filtered = re.sub(
        r"\n*DDL to be executed:\n-+\n.*\Z", "", filtered, flags=re.DOTALL
    )
    filtered = re.sub(r"\n{3,}", "\n\n", filtered)
    filtered = _colorize_diff_markers(filtered)
    print(filtered.rstrip() + "\n")

    # Apply the extension-noise strip + pg_format to the plan file so it
    # matches the migration that `db-revision` would produce. Remove the
    # file entirely when there's nothing actionable left — it'd only be
    # stale noise on the next run.
    sql = ""
    if plan_sql.exists() and plan_sql.stat().st_size > 0:
        sql = re.sub(
            r"^(?:GRANT|REVOKE)[^;]*pg_(?:buffercache|stat_statements)[^;]*;\n?",
            "",
            plan_sql.read_text(),
            flags=re.MULTILINE,
        ).strip()

    if sql:
        plan_sql.write_text(pg_format_sql(sql))
        ok(f"plan DDL → {CYAN}{plan_sql.relative_to(PROJECT_ROOT)}{RESET}")
    else:
        plan_sql.unlink(missing_ok=True)
        info("no DDL to apply")

    if result.stderr:
        print(result.stderr, file=sys.stderr, end="")
    sys.exit(result.returncode)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    command = sys.argv[1]
    commands = {
        "validate": cmd_validate,
        "generate": cmd_generate,
        "check": cmd_check,
        "migrate": cmd_migrate,
        "current": cmd_current,
        "history": cmd_history,
    }

    if command == "revision":
        if len(sys.argv) < 3:
            fail("usage: schema_tool.py revision <name>")
            sys.exit(1)
        cmd_revision(sys.argv[2])
    elif command in commands:
        commands[command]()
    else:
        fail(f"unknown command: {command}")
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
