#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///

# --- How to run ---
# 1. Run with the system Python (no dependencies required):
#      python3 tests/test_model_asset_schema.py
# 2. Or run with uv:
#      uv run tests/test_model_asset_schema.py
# ------------------

from __future__ import annotations

import json
from pathlib import Path
from typing import Final, TypeAlias


REPOSITORY_ROOT: Final = Path(__file__).resolve().parents[1]
AGGREGATE_SCHEMA: Final = REPOSITORY_ROOT / "docs" / "model-asset-manifest.schema.json"
CHILD_SCHEMA_DIRECTORY: Final = REPOSITORY_ROOT / "governance" / "model-assets" / "schemas"
JsonValue: TypeAlias = str | int | float | bool | None | list["JsonValue"] | dict[str, "JsonValue"]


def load_schema(path: Path) -> dict[str, JsonValue]:
    schema = json.loads(path.read_text(encoding="utf-8"))
    assert isinstance(schema, dict)
    return schema


def test_aggregate_schema_references_resolve_from_local_id_catalog() -> None:
    aggregate = load_schema(AGGREGATE_SCHEMA)
    child_schemas = [load_schema(path) for path in sorted(CHILD_SCHEMA_DIRECTORY.glob("*.json"))]
    local_catalog = {schema["$id"]: schema for schema in child_schemas}
    properties = aggregate["properties"]
    assert isinstance(properties, dict)

    for property_schema in properties.values():
        assert isinstance(property_schema, dict)
        assert property_schema["$ref"] in local_catalog


if __name__ == "__main__":
    test_aggregate_schema_references_resolve_from_local_id_catalog()
    print("PASS test_aggregate_schema_references_resolve_from_local_id_catalog")
