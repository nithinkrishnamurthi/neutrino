"""Tests for Neutrino App class."""

from neutrino import App, Route, RouteNotFoundError

try:
    import pytest
except ImportError:
    pytest = None  # type: ignore


def test_route_registration():
    """Test that routes can be registered with decorator."""
    app = App()

    @app.route("/add")
    def add(x, y):
        return x + y

    assert "/add" in app.list_routes()
    route = app.get_route("/add")
    assert isinstance(route, Route)
    assert route.path == "/add"
    assert route.methods == ["GET"]
    assert route(2, 3) == 5


def test_route_with_methods():
    """Test that routes can specify HTTP methods."""
    app = App()

    @app.route("/process", methods=["POST", "PUT"])
    def process(data):
        return {"processed": data}

    route = app.get_route("/process")
    assert route.methods == ["POST", "PUT"]
    assert route({"key": "value"}) == {"processed": {"key": "value"}}


def test_multiple_routes():
    """Test registering multiple routes."""
    app = App()

    @app.route("/add")
    def add(x, y):
        return x + y

    @app.route("/subtract", methods=["POST"])
    def subtract(x, y):
        return x - y

    assert app.list_routes() == ["/add", "/subtract"]
    assert app.get_route("/add")(2, 3) == 5
    assert app.get_route("/subtract")(5, 3) == 2


def test_route_not_found():
    """Test that RouteNotFoundError is raised for missing routes."""
    app = App()

    if pytest:
        with pytest.raises(RouteNotFoundError):
            app.get_route("/nonexistent")
    else:
        try:
            app.get_route("/nonexistent")
            raise AssertionError("Expected RouteNotFoundError")
        except RouteNotFoundError:
            pass


def test_model_registration():
    """Test that models can be registered with decorator."""
    app = App()

    @app.model(name="sentiment", min_replicas=2, max_replicas=5)
    class SentimentModel:
        def load(self):
            pass

        def predict(self, text):
            return "positive"

    assert "sentiment" in app.list_models()
    model = app.get_model("sentiment")
    assert model.config.min_replicas == 2
    assert model.config.max_replicas == 5


def test_route_repr():
    """Test Route string representation."""
    app = App()

    @app.route("/analyze", methods=["GET", "POST"])
    def analyze(text):
        return text

    route = app.get_route("/analyze")
    assert repr(route) == "<Route /analyze [GET,POST]>"


if __name__ == "__main__":
    # Run tests directly (without pytest)
    test_route_registration()
    test_route_with_methods()
    test_multiple_routes()
    test_route_not_found()
    test_model_registration()
    test_route_repr()
    print("All tests passed!")
