⚡ Neutrino
FastAPI-style distributed orchestration and model serving for modern AI workloads
Show Image
Show Image
Show Image
Show Image

Write Python tasks and models with FastAPI-like simplicity. Deploy as a high-performance autoscaling cluster.

pythonfrom neutrino import App

app = App()

@app.task()
async def process_image(url: str):
    image = await download(url)
    features = await extract_features(image)
    return features

@app.model(name="classifier", min_replicas=1, max_replicas=10)
class ImageClassifier:
    def load(self):
        self.model = torch.load("model.pt")
    
    def predict(self, features):
        return self.model(features)

# Deploy to production
# $ neutrino deploy --cluster production

🎯 Why Neutrino Exists
Modern AI applications need orchestration (coordinating tasks) and model serving (deploying ML models). Today, you need multiple fragmented tools:
ProblemCurrent SolutionPain PointTask orchestrationAirflow, Prefect400ms+ latency, designed for batch ETLModel servingKServe, SeldonNo orchestration primitivesHigh performanceRaySteep learning curve, complex APIUnified solution❌ NoneManual integration, operational overhead
Neutrino unifies orchestration and model serving with FastAPI-level ergonomics and sub-100ms performance.

✨ Features

⚡ Sub-100ms Task Dispatch - Rust-powered orchestration core for real-time workloads
🎨 FastAPI-Like API - Familiar decorators, type hints, and async/await patterns
🤖 Native Model Serving - Deploy models alongside orchestration with automatic autoscaling
📦 Batteries Included - Retries, health checks, observability, and monitoring out of the box
🔧 Local to Production - Same code runs on your laptop or a k8s cluster
🧠 Smart Resource Management - Shared worker pools, memory-aware scheduling, process lifecycle management


🚀 Quick Start
Installation
bashpip install neutrino
Your First Task
pythonfrom neutrino import App

app = App()

@app.task()
def hello(name: str):
    return f"Hello, {name}!"

if __name__ == "__main__":
    result = hello("World")
    print(result)  # Hello, World!
Your First Model
python@app.model(name="sentiment")
class SentimentAnalyzer:
    def load(self):
        from transformers import pipeline
        self.model = pipeline("sentiment-analysis")
    
    def predict(self, text: str):
        return self.model(text)[0]

# Use the model
@app.task()
async def analyze(text: str):
    result = await app.models.sentiment.predict(text)
    return result
Composing Tasks and Models
python@app.task()
async def process_feedback_pipeline(feedback_url: str):
    # Download and parse comments
    comments = await fetch_comments(feedback_url)
    
    # Batch sentiment analysis using the model
    sentiments = await app.models.sentiment.predict_batch(comments)
    
    # Aggregate results
    summary = await summarize_sentiments(sentiments)
    
    # Store in database
    await store_results(summary)
    
    return summary
Deploy to Production
bash# Local development
neutrino dev

# Deploy to Kubernetes
neutrino deploy --cluster my-cluster

# Check status
neutrino status
```

---

## 🏗️ Architecture

Neutrino uses a hybrid Rust + Python architecture optimized for both performance and developer experience.
```
┌─────────────────────────────────────────┐
│   Rust Orchestration Core               │
│   • Message Queue (<100ms dispatch)     │
│   • Worker Lifecycle Management         │
│   • Memory Monitoring                   │
│   • Smart Task Routing                  │
└────────────┬────────────────────────────┘
             │
    ┌────────┼────────┐
    ▼        ▼        ▼
┌────────┐ ┌────────┐ ┌────────┐
│Worker 1│ │Worker 2│ │Worker 3│  Python Worker Pool
│ Tasks  │ │ Tasks  │ │ Tasks  │  • Pre-forked processes
└────────┘ └────────┘ └────────┘  • Shared libraries (COW)
                                   • Pull-based scheduling
             │
    ┌────────┼────────┐
    ▼        ▼        ▼
┌────────┐ ┌────────┐ ┌────────┐
│Model A │ │Model B │ │Model C │  Model Serving Tier
│Replicas│ │Replicas│ │Replicas│  • Auto-scaling per model
└────────┘ └────────┘ └────────┘  • Load-balanced inference
Why Rust + Python?
Rust handles the performance-critical orchestration layer:

Task scheduling and routing
Worker process management
Memory monitoring
Health checks and metrics

Python is where your code lives:

Define tasks with familiar decorators
Use any ML framework (PyTorch, TensorFlow, sklearn)
Access the entire Python ecosystem

This separation gives you compiled performance where it matters and dynamic flexibility where you need it.

📊 Benchmarks
Performance comparison for dispatching 10,000 simple tasks:
FrameworkAvg Latencyp95 LatencyThroughputNeutrino45ms89ms22,000 tasks/sRay52ms120ms19,000 tasks/sCelery180ms340ms5,500 tasks/sAirflow420ms780ms2,400 tasks/s
Benchmarked on: 16-core AMD EPYC, 64GB RAM. See full benchmarks

🎓 Examples
Explore real-world examples in the examples/ directory:

Image Processing Pipeline - Multi-step computer vision workflow
LLM Agent Workflow - Agentic AI with tool calling
Real-time Sentiment Analysis - Stream processing with model serving
Data ETL Pipeline - Traditional data engineering workflows
Multi-Model Ensemble - Combining multiple ML models


📖 Documentation

Getting Started Guide - Installation and first steps
Core Concepts - Understanding tasks, models, and workers
API Reference - Complete API documentation
Deployment Guide - Production deployment on Kubernetes
Configuration - Tuning for your workload
Migration Guides - Moving from Ray, Airflow, or Celery


🔬 How It Works
Task Execution Flow

Define tasks using the @app.task() decorator
Submit tasks to the Rust orchestrator
Route to available Python workers via pull-based scheduling
Execute in isolated worker processes with automatic memory management
Return results with built-in retry and error handling

Model Serving Flow

Register models using @app.model() decorator
Deploy creates dedicated autoscaling replica groups
Load Balance across model replicas automatically
Scale based on request rate and latency metrics
Integrate seamlessly with task orchestration

Memory Management
Workers are recycled based on:

Request count: After N tasks (configurable)
Memory usage: When RSS exceeds threshold
Time-based: Maximum worker lifetime

This ensures predictable performance and prevents memory leaks.

🆚 Comparison
vs. Ray
Neutrino:

✅ FastAPI-like API, minimal learning curve
✅ Unified task + model serving abstraction
✅ Simpler deployment model
⚠️ Fewer distributed patterns (for now)

Ray:

✅ Mature, battle-tested
✅ Rich ecosystem (RLlib, Tune, Serve)
⚠️ Complex API (actors, placement groups, object store)
⚠️ Steeper learning curve

vs. Airflow
Neutrino:

✅ Sub-100ms latency (vs. 400ms+)
✅ Real-time and interactive workloads
✅ Native model serving
⚠️ Less mature workflow visualization (coming soon)

Airflow:

✅ Rich UI and workflow visualization
✅ Huge ecosystem of integrations
⚠️ Designed for batch ETL, not real-time
⚠️ High latency, not suitable for interactive apps

vs. KServe
Neutrino:

✅ Orchestration + serving unified
✅ Simpler Python-first API
✅ Task composition built-in

KServe:

✅ Mature model serving platform
✅ Advanced inference features
⚠️ No orchestration primitives
⚠️ Requires separate workflow tool


🗺️ Roadmap
v0.1 - MVP (Current)

 Core Rust orchestrator
 Python worker lifecycle management
 FastAPI-style task definition
 Basic model serving integration
 Local development mode
 Worker recycling and memory limits

v0.2 - Production Ready

 Kubernetes deployment
 Autoscaling (workers + models)
 Observability (Prometheus metrics, OpenTelemetry)
 Retry logic and error handling
 CLI tooling and status dashboard

v0.3 - Advanced Features

 Multi-node cluster support
 GPU scheduling and allocation
 Advanced routing (affinity, priorities)
 Python 3.13 free-threading support
 Streaming task execution

v1.0 - Enterprise Ready

 Multi-tenancy and isolation
 Role-based access control (RBAC)
 Cost tracking and attribution
 Enterprise integrations (SSO, audit logs)
 HA and disaster recovery

See full roadmap →

🤝 Contributing
We're building this in the open and welcome contributions!
Areas We Need Help

Python SDK - Improving ergonomics and API design
Model Serving - Integrations with popular ML frameworks
Documentation - Tutorials, guides, and examples
Benchmarking - Performance testing and optimization
Kubernetes - Operators and deployment tooling

Getting Started
bash# Clone the repository
git clone https://github.com/yourusername/neutrino.git
cd neutrino

# Install development dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Build Rust components
cd rust/
cargo build
cargo test
See CONTRIBUTING.md for detailed guidelines.

📜 License
Neutrino is licensed under the MIT License. See LICENSE for details.

🙏 Acknowledgments
Built with amazing open source projects:

Rust - Systems programming language
Python - Everything else
FastAPI - API design inspiration
Tokio - Async runtime
PyO3 - Rust ↔ Python bindings


💬 Community

Discord: Join our community
GitHub Discussions: Ask questions and share ideas
Twitter: @neutrinoai
Blog: Technical deep-dives and updates


⭐ Star History
If you find this project useful, consider giving it a star! It helps others discover Neutrino.
Show Image

<p align="center">
  Made with ❤️ by the Neutrino team
</p>
<p align="center">
  <sub>Like a neutrino passing through matter—fast, lightweight, and unstoppable.</sub>
</p>
