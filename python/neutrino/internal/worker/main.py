#!/usr/bin/env python3
"""
Neutrino Worker - Python worker process entry point.

Usage: python -m neutrino.internal.worker.main <socket_path> <worker_id>

The worker:
1. Connects to Unix socket at socket_path
2. Sends WorkerReady message
3. Enters main loop waiting for tasks
4. Exits on Shutdown message
"""

import os
import socket
import sys
from typing import NoReturn
import importlib

import msgpack

from neutrino.internal.worker.protocol import ProtocolHandler


def main() -> NoReturn:
    if len(sys.argv) != 4 :
        print(f"Usage: {sys.argv[0]} <socket_path> <worker_id> <app_path>", file=sys.stderr)
        sys.exit(1)

    socket_path = sys.argv[1]
    worker_id = sys.argv[2]
    app_path = sys.argv[3]
    pid = os.getpid()

    print(f"[Worker {worker_id}] Starting (pid={pid})")

    # Connect to Unix socket
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(socket_path)
        print(f"[Worker {worker_id}] Connected to {socket_path}")
    except Exception as e:
        print(f"[Worker {worker_id}] Failed to connect: {e}", file=sys.stderr)
        sys.exit(1)

    protocol = ProtocolHandler(sock)

    # Send ready message
    protocol.send_ready(worker_id, pid)
    print(f"[Worker {worker_id}] Sent ready message")

    # Main message loop
    try:
        while True:
            message = protocol.recv()
            print(f"[Worker {worker_id}] Received: {message}")

            # Handle different message types
            if "Shutdown" in message:
                shutdown_data = message["Shutdown"]
                # Handle both dict format and tuple/list format from msgpack
                if isinstance(shutdown_data, dict):
                    graceful = shutdown_data.get("graceful", True)
                elif isinstance(shutdown_data, (list, tuple)):
                    graceful = shutdown_data[0] if shutdown_data else True
                else:
                    graceful = bool(shutdown_data)
                print(f"[Worker {worker_id}] Shutting down (graceful={graceful})")
                break
            elif "TaskAssignment" in message:
                # TODO: Execute task
                task_data = message["TaskAssignment"]

                # Handle both dict format and tuple/list format from msgpack
                if isinstance(task_data, dict):
                    task_id = task_data["task_id"]
                    func_name = task_data["function_name"]
                    args = task_data["args"]  # Already decoded as native structure
                elif isinstance(task_data, (list, tuple)):
                    # Rust serializes as tuple: [task_id, function_name, args]
                    task_id = task_data[0]
                    func_name = task_data[1]
                    args = task_data[2]  # Already decoded as native structure
                else:
                    print(f"[Worker {worker_id}] Error: unexpected TaskAssignment format: {type(task_data)}")
                    continue

                print(f"[Worker {worker_id}] Task {task_id}: {func_name}({args})")

                # For now, just acknowledge with a simple response
                # Result is sent as native msgpack value (no double encoding)
                result = {"status": "ok", "message": "Task received"}
                protocol.send_task_result(task_id, True, result)
            elif "Heartbeat" in message:
                # Respond to heartbeat
                protocol.send_heartbeat(worker_id)
            else:
                print(f"[Worker {worker_id}] Unknown message: {message}")

    except Exception as e:
        print(f"[Worker {worker_id}] Error: {e}", file=sys.stderr)
    finally:
        sock.close()
        print(f"[Worker {worker_id}] Exiting")

    sys.exit(0)


if __name__ == "__main__":
    main()
