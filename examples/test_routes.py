"""
Example Neutrino app demonstrating exact route matching via OpenAPI.

This example shows how routes defined with @app.route() decorator
are transformed into exact HTTP routes (not generic task patterns).
"""

import sys
sys.path.insert(0, "/home/nithin/neutrino/python")

from neutrino import App

# Create the app
app = App()


@app.route("/api/users", methods=["GET"])
def list_users():
    """List all users."""
    return {"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]}


@app.route("/api/users/{user_id}", methods=["GET"])
def get_user(user_id: int):
    """Get a specific user by ID."""
    return {"id": user_id, "name": f"User {user_id}"}


@app.route("/api/products", methods=["GET", "POST"])
def manage_products(action: str):
    """List or create products."""
    if action == "list":
        return {"products": [{"id": 1, "name": "Widget"}]}
    else:
        return {"created": True}


@app.route("/api/health", methods=["GET"])
def health():
    """Health check endpoint."""
    return {"status": "healthy", "service": "example-app"}


if __name__ == "__main__":
    # Generate OpenAPI spec
    import json
    spec = app.generate_openapi(title="Example API", version="1.0.0")

    print("Generated OpenAPI Specification:")
    print(json.dumps(spec, indent=2))

    print("\n" + "="*60)
    print("Routes registered:")
    for path in app.list_routes():
        route = app.get_route(path)
        methods = ", ".join(route.methods)
        print(f"  {methods:10s} {path:30s} -> {route.handler.__name__}")
