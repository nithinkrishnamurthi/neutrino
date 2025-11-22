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
import asyncio
import inspect

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

    # Import the app module at startup (pre-fork pattern - stays hot)
    print(f"[Worker {worker_id}] Loading app module: {app_path}")
    try:
        module_parts = app_path.rsplit(":", 1)
        module_name = module_parts[0]
        app_var_name = module_parts[1] if len(module_parts) > 1 else "app"

        # Import the module
        module = importlib.import_module(module_name)
        app = getattr(module, app_var_name)
        print(f"[Worker {worker_id}] App loaded successfully with {len(app._route_registry)} routes")
    except Exception as e:
        print(f"[Worker {worker_id}] Failed to load app: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)

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
                    protocol.send_task_result(task_id, False, {"error": "Invalid task format"})
                    continue

                print(f"[Worker {worker_id}] Task {task_id}: {func_name}({args})")

                # Execute the task using pre-loaded app
                try:
                    # Find the route handler by function name
                    route = None
                    for path, route_obj in app._route_registry.items():
                        if route_obj.handler.__name__ == func_name:
                            route = route_obj
                            break

                    if route is None:
                        raise ValueError(f"Route handler '{func_name}' not found")

                    # Prepare arguments based on route's request_model
                    if route.request_model is not None:
                        # If there's a Pydantic model, instantiate it from args dict
                        if args and isinstance(args, dict):
                            request_obj = route.request_model(**args)
                            result = route.handler(request_obj)
                        else:
                            # Empty args, create model with no data
                            request_obj = route.request_model()
                            result = route.handler(request_obj)
                    else:
                        # No request model - pass args as kwargs or call with no args
                        if args and isinstance(args, dict) and args:
                            result = route.handler(**args)
                        else:
                            result = route.handler()

                    # If result is a coroutine (async function), await it
                    if inspect.iscoroutine(result):
                        result = asyncio.run(result)

                    # Convert result to dict if it's a Pydantic model
                    if hasattr(result, 'model_dump'):
                        # Pydantic v2
                        result_dict = result.model_dump()
                    elif hasattr(result, 'dict'):
                        # Pydantic v1
                        result_dict = result.dict()
                    else:
                        # Plain dict or other serializable type
                        result_dict = result

                    print(f"[Worker {worker_id}] Task {task_id} succeeded: {result_dict}")
                    protocol.send_task_result(task_id, True, result_dict)

                except Exception as e:
                    print(f"[Worker {worker_id}] Task {task_id} failed: {e}", file=sys.stderr)
                    import traceback
                    traceback.print_exc()
                    error_msg = {"error": str(e), "type": type(e).__name__}
                    protocol.send_task_result(task_id, False, error_msg)
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
