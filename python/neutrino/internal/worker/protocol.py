"""
Protocol handler for communication with Rust orchestrator.
Wire format: [4 bytes: big-endian length][N bytes: msgpack payload]
"""

import socket
import struct
from typing import Any

import msgpack


class ProtocolHandler:
    """Handles msgpack communication over Unix socket."""

    def __init__(self, sock: socket.socket):
        self.sock = sock

    def send(self, message: dict[str, Any]) -> None:
        """Send a message to the orchestrator."""
        payload = msgpack.packb(message, use_bin_type=True)
        length = struct.pack(">I", len(payload))  # Big-endian u32
        self.sock.sendall(length + payload)

    def recv(self) -> dict[str, Any]:
        """Receive a message from the orchestrator."""
        # Read length prefix (4 bytes, big-endian)
        length_bytes = self._recv_exact(4)
        length = struct.unpack(">I", length_bytes)[0]

        # Read payload
        payload = self._recv_exact(length)
        return msgpack.unpackb(payload, raw=False)

    def _recv_exact(self, n: int) -> bytes:
        """Receive exactly n bytes from socket."""
        data = b""
        while len(data) < n:
            chunk = self.sock.recv(n - len(data))
            if not chunk:
                raise ConnectionError("Socket closed")
            data += chunk
        return data

    def send_ready(self, worker_id: str, pid: int) -> None:
        """Send WorkerReady message."""
        # Match Rust enum variant structure for msgpack
        self.send({"WorkerReady": {"worker_id": worker_id, "pid": pid}})

    def send_task_result(
        self, task_id: str, success: bool, result: Any
    ) -> None:
        """Send TaskResult message.

        Args:
            task_id: Unique task identifier
            success: Whether task succeeded
            result: Native Python value (dict, list, str, int, etc.) - will be encoded as msgpack
        """
        self.send(
            {
                "TaskResult": {
                    "task_id": task_id,
                    "success": success,
                    "result": result,  # Native value, encoded once with entire message
                }
            }
        )

    def send_heartbeat(self, worker_id: str) -> None:
        """Send Heartbeat message."""
        self.send({"Heartbeat": {"worker_id": worker_id}})