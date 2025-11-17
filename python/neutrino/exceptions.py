"""
Neutrino exceptions.
"""


class NeutrinoError(Exception):
    """Base exception for all Neutrino errors."""
    pass


class RouteError(NeutrinoError):
    """Error during route execution."""
    pass


class RouteNotFoundError(RouteError):
    """Route not found in registry."""
    pass


class ModelError(NeutrinoError):
    """Error during model operations."""
    pass


class ModelNotFoundError(ModelError):
    """Model not found in registry."""
    pass


class WorkerError(NeutrinoError):
    """Error in worker process."""
    pass


class ProtocolError(NeutrinoError):
    """Error in communication protocol."""
    pass
