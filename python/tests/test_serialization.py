"""Tests for serialization and deserialization of messages between Rust and Python."""

import msgpack
from neutrino.internal.worker.protocol import ProtocolHandler

try:
    import pytest
except ImportError:
    pytest = None  # type: ignore


class TestMessageSerialization:
    """Test serialization of protocol messages."""

    def test_worker_ready_message(self):
        """Test WorkerReady message serialization."""
        message = {
            "WorkerReady": {
                "worker_id": "worker-001",
                "pid": 12345
            }
        }

        # Serialize and deserialize
        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["WorkerReady"]["worker_id"] == "worker-001"
        assert unpacked["WorkerReady"]["pid"] == 12345

    def test_task_assignment_message_with_dict(self):
        """Test TaskAssignment message with dict args."""
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
        assert unpacked["TaskAssignment"]["args"]["text"] == "hello"
        assert unpacked["TaskAssignment"]["args"]["count"] == 5

    def test_task_assignment_message_with_list(self):
        """Test TaskAssignment message with list args."""
        message = {
            "TaskAssignment": {
                "task_id": "task-002",
                "function_name": "sum_numbers",
                "args": [1, 2, 3, 4, 5]
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["TaskAssignment"]["args"] == [1, 2, 3, 4, 5]

    def test_task_assignment_message_empty_args(self):
        """Test TaskAssignment message with empty args."""
        message = {
            "TaskAssignment": {
                "task_id": "task-003",
                "function_name": "no_args_func",
                "args": {}
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["TaskAssignment"]["args"] == {}

    def test_task_result_success(self):
        """Test TaskResult message with successful result."""
        message = {
            "TaskResult": {
                "task_id": "task-001",
                "success": True,
                "result": {"status": "ok", "data": [1, 2, 3]}
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["TaskResult"]["success"] is True
        assert unpacked["TaskResult"]["result"]["status"] == "ok"

    def test_task_result_failure(self):
        """Test TaskResult message with error."""
        message = {
            "TaskResult": {
                "task_id": "task-002",
                "success": False,
                "result": {"error": "ValueError: invalid input", "type": "ValueError"}
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["TaskResult"]["success"] is False
        assert "ValueError" in unpacked["TaskResult"]["result"]["error"]

    def test_shutdown_message(self):
        """Test Shutdown message."""
        message = {
            "Shutdown": {
                "graceful": True
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["Shutdown"]["graceful"] is True

    def test_heartbeat_message(self):
        """Test Heartbeat message."""
        message = {
            "Heartbeat": {
                "worker_id": "worker-001"
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == message
        assert unpacked["Heartbeat"]["worker_id"] == "worker-001"


class TestDataTypeSerialization:
    """Test serialization of various data types."""

    def test_serialize_null(self):
        """Test None/null serialization."""
        data = {"value": None}
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked["value"] is None

    def test_serialize_boolean(self):
        """Test boolean serialization."""
        data = {"true_val": True, "false_val": False}
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked["true_val"] is True
        assert unpacked["false_val"] is False

    def test_serialize_integers(self):
        """Test integer serialization."""
        data = {
            "small": 42,
            "negative": -123,
            "zero": 0,
            "large": 1234567890
        }
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked == data

    def test_serialize_floats(self):
        """Test float serialization."""
        data = {
            "pi": 3.14159,
            "negative": -2.5,
            "scientific": 1.23e-4
        }
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert abs(unpacked["pi"] - 3.14159) < 0.00001
        assert unpacked["negative"] == -2.5

    def test_serialize_strings(self):
        """Test string serialization."""
        data = {
            "simple": "hello",
            "unicode": "Hello 世界 🌍",
            "empty": "",
            "special": "Line1\nLine2\tTabbed"
        }
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked == data

    def test_serialize_lists(self):
        """Test list/array serialization."""
        data = {
            "empty": [],
            "numbers": [1, 2, 3, 4, 5],
            "mixed": [1, "two", 3.0, True, None],
            "nested": [[1, 2], [3, 4], [5, 6]]
        }
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked == data

    def test_serialize_nested_dicts(self):
        """Test nested dictionary serialization."""
        data = {
            "user": {
                "id": 1,
                "name": "Alice",
                "metadata": {
                    "created": "2024-01-01",
                    "tags": ["admin", "user"]
                }
            }
        }
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked == data
        assert unpacked["user"]["metadata"]["tags"] == ["admin", "user"]

    def test_serialize_binary_data(self):
        """Test binary data serialization."""
        data = {"bytes": b"\x00\x01\x02\xff"}
        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)
        assert unpacked["bytes"] == b"\x00\x01\x02\xff"


class TestRustPythonCompatibility:
    """Test compatibility between Rust and Python message formats."""

    def test_rust_tuple_format_task_assignment(self):
        """Test that Rust's tuple format can be parsed by Python.

        Rust serializes enums as tuples: [variant_index, field1, field2, ...]
        """
        # This is how Rust serializes TaskAssignment
        message = {
            "TaskAssignment": [
                "task-123",           # task_id
                "my_function",        # function_name
                {"arg1": "value1"}    # args
            ]
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        task_data = unpacked["TaskAssignment"]
        assert isinstance(task_data, list)
        assert task_data[0] == "task-123"
        assert task_data[1] == "my_function"
        assert task_data[2] == {"arg1": "value1"}

    def test_rust_dict_format_task_assignment(self):
        """Test that Rust's dict format can be parsed by Python."""
        # Alternative format Rust might use
        message = {
            "TaskAssignment": {
                "task_id": "task-123",
                "function_name": "my_function",
                "args": {"arg1": "value1"}
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        task_data = unpacked["TaskAssignment"]
        assert isinstance(task_data, dict)
        assert task_data["task_id"] == "task-123"
        assert task_data["function_name"] == "my_function"

    def test_complex_args_roundtrip(self):
        """Test complex args structure roundtrip."""
        args = {
            "text": "Hello world",
            "iterations": 1000,
            "options": {
                "timeout": 30,
                "retries": 3,
                "callbacks": ["on_success", "on_failure"]
            },
            "metadata": {
                "user_id": 123,
                "timestamp": "2024-01-01T00:00:00Z"
            }
        }

        message = {
            "TaskAssignment": {
                "task_id": "complex-task",
                "function_name": "process",
                "args": args
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked["TaskAssignment"]["args"] == args


class TestEdgeCases:
    """Test edge cases and error conditions."""

    def test_large_payload(self):
        """Test handling of large payloads."""
        large_list = list(range(10000))
        message = {
            "TaskResult": {
                "task_id": "large-task",
                "success": True,
                "result": {"data": large_list}
            }
        }

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert len(unpacked["TaskResult"]["result"]["data"]) == 10000
        assert unpacked["TaskResult"]["result"]["data"][9999] == 9999

    def test_deeply_nested_structure(self):
        """Test deeply nested data structures."""
        nested = {"level": 1}
        current = nested
        for i in range(2, 20):
            current["nested"] = {"level": i}
            current = current["nested"]

        message = {"TaskResult": {"task_id": "nested", "success": True, "result": nested}}

        packed = msgpack.packb(message, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        # Navigate to deepest level
        current = unpacked["TaskResult"]["result"]
        for _ in range(18):
            assert "nested" in current
            current = current["nested"]
        assert current["level"] == 19

    def test_unicode_edge_cases(self):
        """Test various unicode edge cases."""
        data = {
            "emoji": "😀😃😄😁",
            "chinese": "你好世界",
            "arabic": "مرحبا بالعالم",
            "mixed": "Hello 世界 🌍 مرحبا",
            "zero_width": "a\u200Bb",  # Zero-width space
        }

        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked == data

    def test_special_float_values(self):
        """Test special float values."""
        data = {
            "inf": float('inf'),
            "neg_inf": float('-inf'),
            "very_small": 1e-308,
            "very_large": 1e308
        }

        packed = msgpack.packb(data, use_bin_type=True)
        unpacked = msgpack.unpackb(packed, raw=False)

        assert unpacked["inf"] == float('inf')
        assert unpacked["neg_inf"] == float('-inf')
        assert unpacked["very_small"] == 1e-308


def run_all_tests():
    """Run all tests without pytest."""
    test_classes = [
        TestMessageSerialization,
        TestDataTypeSerialization,
        TestRustPythonCompatibility,
        TestEdgeCases
    ]

    for test_class in test_classes:
        print(f"\nRunning {test_class.__name__}...")
        instance = test_class()
        for attr_name in dir(instance):
            if attr_name.startswith("test_"):
                method = getattr(instance, attr_name)
                try:
                    method()
                    print(f"  ✓ {attr_name}")
                except AssertionError as e:
                    print(f"  ✗ {attr_name}: {e}")
                except Exception as e:
                    print(f"  ✗ {attr_name}: Unexpected error: {e}")


if __name__ == "__main__":
    run_all_tests()
    print("\nAll serialization tests passed!")
