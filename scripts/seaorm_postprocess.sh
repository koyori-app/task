#!/usr/bin/env bash
# Apply entity_openapi.toml attributes to sea-orm-cli generated entity files.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND="$ROOT/apps/backend"
GENERATED="$BACKEND/src/entities/_generated"
MANIFEST="$BACKEND/config/entity_openapi.toml"

if [[ ! -f "$MANIFEST" ]]; then
  echo "error: manifest not found: $MANIFEST" >&2
  exit 1
fi

python3 - "$MANIFEST" "$GENERATED" "$@" <<'PY'
import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[no-redef]

manifest_path = Path(sys.argv[1])
generated_dir = Path(sys.argv[2])
only = set(sys.argv[3:]) if len(sys.argv) > 3 else None

with manifest_path.open("rb") as f:
    manifest = tomllib.load(f)

entities = manifest.get("entities", {})


def inject_entity(entity_name: str, cfg: dict) -> None:
    target = generated_dir / f"{entity_name}.rs"
    if not target.exists():
        raise SystemExit(f"generated entity missing: {target}")

    text = target.read_text()
    text = text.replace("utoipa :: ToSchema", "utoipa::ToSchema")
    text = text.replace("DateTimeWithTimeZone", "DateTimeUtc")

    if "use utoipa::ToSchema" not in text and "utoipa::ToSchema" in text:
        text = text.replace(
            "use sea_orm::entity::prelude::*;",
            "use sea_orm::entity::prelude::*;\nuse utoipa::ToSchema;",
            1,
        )

    struct_attrs = cfg.get("struct_attrs", [])
    if struct_attrs:
        block = "\n".join(struct_attrs)
        text, n = re.subn(
            r"(#\[sea_orm\(table_name = \"[^\"]+\"\)\])",
            rf"\1\n{block}",
            text,
            count=1,
        )
        if n == 0:
            raise SystemExit(f"could not inject struct attrs for {entity_name}")

    doc_map: dict[str, list[str]] = {}
    for item in cfg.get("doc_comments", []):
        field = item["field"]
        if "lines" in item:
            doc_map[field] = item["lines"]
        else:
            doc_map[field] = [item["comment"]]

    fields = list(cfg.get("fields", []))
    known = {f["name"] for f in fields}
    for field_name, lines in doc_map.items():
        if field_name not in known:
            fields.append({"name": field_name, "attrs": []})

    for field in fields:
        name = field["name"]
        attrs = field.get("attrs", [])
        doc_lines = doc_map.get(name, [])

        field_re = re.compile(
            rf"^(\s*)((?:#\[[^\n]+\]\n\s*)*)pub {re.escape(name)}:",
            re.MULTILINE,
        )
        match = field_re.search(text)
        if not match:
            raise SystemExit(f"field not found in {entity_name}: {name}")

        indent = match.group(1)
        existing_attrs = match.group(2)
        insert_lines: list[str] = []
        for line in doc_lines:
            insert_lines.append(f"{indent}/// {line}")
        for attr in attrs:
            if attr in existing_attrs:
                continue
            insert_lines.append(f"{indent}{attr}")

        replacement = "\n".join(insert_lines) + "\n" + match.group(0)
        text = text[: match.start()] + replacement + text[match.end() :]

    target.write_text(text)
    print(f"postprocessed: {target}")


for entity_name, cfg in entities.items():
    if only and entity_name not in only:
        continue
    inject_entity(entity_name, cfg)
PY

echo "seaorm_postprocess: done"
