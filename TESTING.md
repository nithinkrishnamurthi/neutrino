# Testing Guide

This document describes the test suite for Neutrino's serialization and messaging system.

## Overview

Neutrino uses msgpack for efficient binary serialization between Rust and Python components. The test suite ensures that data can be correctly converted and transmitted across the system.

## Test Files

### Python Tests

**Location**: `python/tests/test_serialization.py`

Tests the Python side of serialization:
- Protocol message format (WorkerReady, TaskAssignment, TaskResult, etc.)
- Data type serialization (null, bool, int, float, string, array, dict)
- Rust-Python compatibility (tuple vs dict formats)
- Edge cases (large payloads, deeply nested structures, unicode)

**Run tests:**
```bash
# With pytest
pytest python/tests/test_serialization.py -v

# Without pytest (direct execution)
python3 python/tests/test_serialization.py
```

### Rust Tests

**Location**: `crates/neutrino-core/tests/serialization_tests.rs`

Tests the Rust side of JSON ↔ msgpack conversion:
- Roundtrip conversions (JSON → msgpack → JSON)
- Data type conversions matching HTTP handler logic
- Task request/response conversions
- Edge cases (binary data, large arrays, deep nesting, unicode)

**Run tests:**
```bash
# Run all serialization tests
cargo test --test serialization_tests

# Run with output
cargo test --test serialization_tests -- --nocapture

# Run specific test
cargo test --test serialization_tests json_msgpack_conversion::test_task_request_conversion
```

## Test Coverage

### Message Types Tested

✅ **WorkerReady** - Worker announces readiness
```python
{"WorkerReady": {"worker_id": "worker-001", "pid": 12345}}
```

✅ **TaskAssignment** - Orchestrator assigns task to worker
```python
{"TaskAssignment": {"task_id": "task-001", "function_name": "process", "args": {...}}}
# Also supports Rust tuple format:
{"TaskAssignment": ["task-001", "process", {...}]}
```

✅ **TaskResult** - Worker reports task completion
```python
{"TaskResult": {"task_id": "task-001", "success": true, "result": {...}}}
```

✅ **Shutdown** - Orchestrator requests worker shutdown
```python
{"Shutdown": {"graceful": true}}
```

✅ **Heartbeat** - Health check ping
```python
{"Heartbeat": {"worker_id": "worker-001"}}
```

### Data Types Tested

| Type | Rust | Python | Notes |
|------|------|--------|-------|
| Null | `Value::Null` | `None` | ✅ Roundtrip |
| Boolean | `Value::Bool` | `True/False` | ✅ Roundtrip |
| Integer | `Value::Number` | `int` | ✅ Roundtrip (i64/u64) |
| Float | `Value::Number` | `float` | ✅ Roundtrip (f32/f64) |
| String | `Value::String` | `str` | ✅ Unicode support |
| Array | `Value::Array` | `list` | ✅ Nested arrays |
| Object | `Value::Object` | `dict` | ✅ Nested objects |
| Binary | N/A | `bytes` | ✅ Converts to int array in JSON |

### Edge Cases Tested

✅ **Large payloads** - 10,000 element arrays
✅ **Deep nesting** - 50+ levels of nested objects
✅ **Unicode** - Emoji, Chinese, Arabic, mixed scripts
✅ **Empty structures** - Empty dicts, arrays, strings
✅ **Special floats** - Infinity, very small/large numbers
✅ **Binary data** - Byte arrays converted to int arrays
✅ **Invalid inputs** - Non-string map keys (error handling)

## Example Test Cases

### Python: Task Assignment Roundtrip

```python
def test_task_assignment_message_with_dict():
    message = {
        "TaskAssignment": {
            "task_id": "task-001",
            "function_name": "process_data",
            "args": {"text": "hello", "count": 5}
        }
    }

    packed = msgpack.packb(message, use_bin_type=True)
    unpacked = msgpack.unpackb(packed, raw=False)

    assert unpacked == message
```

### Rust: JSON to Msgpack Conversion

```rust
#[test]
fn test_task_request_conversion() {
    let json = serde_json::json!({
        "text": "hello world",
        "iterations": 1000,
        "options": {
            "timeout": 30,
            "retries": 3
        }
    });

    let msgpack = json_to_msgpack_value(&json).unwrap();
    let back_to_json = msgpack_value_to_json(&msgpack).unwrap();

    assert_eq!(json, back_to_json);
}
```

## Running All Tests

```bash
# Run Python tests
python3 python/tests/test_serialization.py

# Run Rust tests
cargo test --test serialization_tests

# Run all tests (Python + Rust)
python3 python/tests/test_serialization.py && cargo test --test serialization_tests
```

## Test Output

### Python Tests
```
Running TestMessageSerialization...
  ✓ test_worker_ready_message
  ✓ test_task_assignment_message_with_dict
  ✓ test_task_result_success
  ...

All serialization tests passed!
```

### Rust Tests
```
running 17 tests
test json_msgpack_conversion::test_null_roundtrip ... ok
test json_msgpack_conversion::test_boolean_roundtrip ... ok
test json_msgpack_conversion::test_task_request_conversion ... ok
...

test result: ok. 17 passed; 0 failed; 0 ignored
```

## Integration Testing

The serialization tests complement the integration tests:

1. **Unit tests** (this file) - Test serialization in isolation
2. **Worker tests** - Test actual message passing over Unix sockets
3. **HTTP tests** - Test end-to-end request/response flow

## Continuous Integration

These tests should be run in CI:

```yaml
# .github/workflows/test.yml
- name: Run Python serialization tests
  run: python3 python/tests/test_serialization.py

- name: Run Rust serialization tests
  run: cargo test --test serialization_tests
```

## Troubleshooting

### Common Issues

**"can not serialize 'X' object"**
- Check that the object is JSON-serializable
- For custom objects, convert to dict first

**"Map keys must be strings"**
- JSON requires string keys in objects
- Convert non-string keys to strings before serialization

**Unicode errors**
- Ensure `use_bin_type=True` in msgpack.packb()
- Ensure `raw=False` in msgpack.unpackb()

**Infinity/NaN handling**
- JSON doesn't officially support infinity
- These may serialize to null depending on settings

## Adding New Tests

When adding new message types or data structures:

1. Add Python test in `test_serialization.py`
2. Add Rust test in `serialization_tests.rs`
3. Test both dict and tuple formats (for Rust enum compatibility)
4. Test roundtrip conversion (serialize → deserialize → compare)
5. Test edge cases (empty, large, nested, unicode)

Example:

```python
# Python
def test_new_message_type():
    message = {"NewMessage": {"field": "value"}}
    packed = msgpack.packb(message, use_bin_type=True)
    unpacked = msgpack.unpackb(packed, raw=False)
    assert unpacked == message
```

```rust
// Rust
#[test]
fn test_new_data_structure() {
    let json = serde_json::json!({"field": "value"});
    let msgpack = json_to_msgpack_value(&json).unwrap();
    let back = msgpack_value_to_json(&msgpack).unwrap();
    assert_eq!(json, back);
}
```
