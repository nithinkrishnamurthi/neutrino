"""
Test script to verify per-task resource tracking implementation.
"""

import sys
import json

# Import the example
sys.path.insert(0, '/home/nithin/neutrino')
import examples.gpu_resources

# Import neutrino
from neutrino import generate_openapi, list_routes

def test_resource_spec_generation():
    """Test that OpenAPI spec includes resource requirements."""
    print("=" * 60)
    print("Testing Resource-Aware OpenAPI Spec Generation")
    print("=" * 60)

    # Generate OpenAPI spec
    spec = generate_openapi(title="GPU Resources Test", version="1.0.0")

    # Check routes are registered
    routes = list_routes()
    print(f"\n✓ Registered {len(routes)} routes:")
    for route in routes:
        print(f"  - {route}")

    # Check resource requirements in spec
    print("\n" + "=" * 60)
    print("Resource Requirements in OpenAPI Spec:")
    print("=" * 60)

    for path, path_item in spec.get("paths", {}).items():
        for method, operation in path_item.items():
            if method in ["get", "post", "put", "patch", "delete"]:
                resources = operation.get("x-neutrino-resources")
                if resources:
                    print(f"\n{method.upper()} {path}")
                    print(f"  CPUs:      {resources['num_cpus']}")
                    print(f"  GPUs:      {resources['num_gpus']}")
                    print(f"  Memory:    {resources['memory_gb']} GB")

    # Save spec to file
    with open("/home/nithin/neutrino/openapi_test.json", "w") as f:
        json.dump(spec, f, indent=2)

    print("\n" + "=" * 60)
    print("✓ OpenAPI spec saved to openapi_test.json")
    print("=" * 60)

    # Verify resource requirements exist
    found_resources = False
    for path, path_item in spec.get("paths", {}).items():
        for method, operation in path_item.items():
            if "x-neutrino-resources" in operation:
                found_resources = True
                break
        if found_resources:
            break

    if found_resources:
        print("\n✅ SUCCESS: Resource requirements are included in OpenAPI spec!")
    else:
        print("\n❌ FAILURE: No resource requirements found in OpenAPI spec!")
        return False

    return True

if __name__ == "__main__":
    success = test_resource_spec_generation()
    sys.exit(0 if success else 1)
