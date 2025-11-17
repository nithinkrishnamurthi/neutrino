"""
Model registration and serving configuration.
"""

from typing import Any, Type


class ModelConfig:
    """Configuration for a deployed model."""

    def __init__(
        self,
        name: str,
        cls: Type[Any],
        min_replicas: int = 1,
        max_replicas: int = 10,
    ):
        self.name = name
        self.cls = cls
        self.min_replicas = min_replicas
        self.max_replicas = max_replicas

    def __repr__(self) -> str:
        return f"<ModelConfig {self.name} replicas={self.min_replicas}-{self.max_replicas}>"


class Model:
    """Represents a registered model."""

    def __init__(self, config: ModelConfig):
        self.config = config
        self.name = config.name

    def __repr__(self) -> str:
        return f"<Model {self.name}>"
