"""
Example demonstrating GPU and CPU resource affinity.

This example shows how to specify resource requirements for routes,
similar to Ray's resource specification approach.
"""

from neutrino import route

@route(
    "/api/preprocess",
    methods=["POST"],
    summary="Lightweight preprocessing",
    description="Preprocess data with minimal CPU requirements",
    num_cpus=0.5,  # Can share a CPU core
    num_gpus=0.0,  # No GPU needed
    memory_gb=0.5
)
def preprocess_data(data: dict):
    """Lightweight data preprocessing that doesn't need a full CPU core."""
    # Example: normalize, clean data
    return {
        "status": "preprocessed",
        "data": data,
        "resources": {
            "cpu": 0.5,
            "gpu": 0.0,
            "memory_gb": 0.5
        }
    }


@route(
    "/api/cpu-intensive",
    methods=["POST"],
    summary="CPU-intensive computation",
    description="Heavy computation requiring multiple CPU cores",
    num_cpus=4.0,  # Needs 4 CPU cores
    num_gpus=0.0,  # No GPU
    memory_gb=8.0
)
def heavy_computation(data: dict):
    """CPU-intensive task like data aggregation or complex calculations."""
    return {
        "status": "computed",
        "result": "...",
        "resources": {
            "cpu": 4.0,
            "gpu": 0.0,
            "memory_gb": 8.0
        }
    }


@route(
    "/api/inference",
    methods=["POST"],
    summary="GPU-accelerated inference",
    description="Run ML model inference on GPU",
    num_cpus=2.0,  # 2 CPU cores for data loading
    num_gpus=1.0,  # 1 full GPU
    memory_gb=16.0  # 16GB for model weights
)
def run_inference(request: dict):
    """
    Run model inference on GPU.

    This will only be scheduled on workers that have:
    - At least 2 CPU cores available
    - At least 1 GPU available
    - At least 16GB memory available
    """
    # Example: load model, run inference
    return {
        "status": "inference_complete",
        "predictions": [...],
        "resources": {
            "cpu": 2.0,
            "gpu": 1.0,
            "memory_gb": 16.0
        }
    }


@route(
    "/api/multi-gpu",
    methods=["POST"],
    summary="Multi-GPU training",
    description="Distributed training across multiple GPUs",
    num_cpus=8.0,   # 8 cores for data loading
    num_gpus=4.0,   # 4 GPUs for distributed training
    memory_gb=64.0  # 64GB for large batches
)
def distributed_training(config: dict):
    """
    Distributed training task requiring multiple GPUs.

    This will only run on workers with:
    - 8+ CPU cores
    - 4+ GPUs
    - 64+ GB memory
    """
    return {
        "status": "training_started",
        "config": config,
        "resources": {
            "cpu": 8.0,
            "gpu": 4.0,
            "memory_gb": 64.0
        }
    }


@route(
    "/api/fractional-gpu",
    methods=["POST"],
    summary="Fractional GPU usage",
    description="Share a GPU with other tasks",
    num_cpus=1.0,
    num_gpus=0.25,  # 1/4 of a GPU (for small models)
    memory_gb=4.0
)
def small_model_inference(data: dict):
    """
    Small model inference that doesn't need a full GPU.

    Multiple tasks with num_gpus=0.25 can share the same GPU,
    similar to Ray's fractional resource allocation.
    """
    return {
        "status": "complete",
        "result": "...",
        "resources": {
            "cpu": 1.0,
            "gpu": 0.25,
            "memory_gb": 4.0
        }
    }
