# Neutrino Kubernetes Deployment

This directory contains Kubernetes manifests for deploying Neutrino on k3s or any Kubernetes cluster.

## Architecture

Neutrino uses an **orchestrator-per-pod** architecture:

```
┌─────────────────────────────────┐
│     k3s LoadBalancer            │
│        :8080                    │
└────────────┬────────────────────┘
             │
    ┌────────┴────────┐
    │                 │
    ▼                 ▼
┌─────────┐       ┌─────────┐
│  Pod 1  │       │  Pod 2  │
│ ┌─────┐ │       │ ┌─────┐ │
│ │Rust │ │       │ │Rust │ │
│ │Orch.│ │       │ │Orch.│ │
│ │  ↕  │ │       │ │  ↕  │ │
│ │4xPy │ │       │ │4xPy │ │
│ │Work │ │       │ │Work │ │
│ └─────┘ │       │ └─────┘ │
└─────────┘       └─────────┘
```

Each pod contains:
- 1 Rust orchestrator process
- 4 Python worker processes (configurable)
- Communication via Unix domain sockets (within pod)

## Quick Start

### Prerequisites

- k3s or Kubernetes cluster running
- kubectl configured
- Docker installed
- Python 3.11+

### Deploy

```bash
# From project root
./scripts/deploy.sh
```

This script will:
1. Generate OpenAPI spec from your Neutrino app
2. Build Docker image
3. Import image to k3s
4. Deploy to Kubernetes
5. Wait for pods to be ready

### Verify Deployment

```bash
# Check pods
kubectl get pods -l app=neutrino

# Check service
kubectl get service neutrino

# View logs
kubectl logs -f deployment/neutrino

# Test health endpoint
curl http://localhost:8080/health
```

## Manual Deployment

If you prefer to deploy manually:

### 1. Generate OpenAPI Spec

```bash
python -m cli.main deploy examples.fastapi_integration --openapi
```

This creates:
- `openapi.json` - Route definitions for Rust router
- `uvicorn_app.py` - ASGI app startup script

### 2. Build Docker Image

```bash
docker build -t neutrino:latest .
```

### 3. Import to k3s

```bash
docker save neutrino:latest -o /tmp/neutrino.tar
sudo k3s ctr images import /tmp/neutrino.tar
```

### 4. Create ConfigMaps

```bash
# Create ConfigMap for generated app code
kubectl create configmap neutrino-app-code \
  --from-file=openapi.json=openapi.json \
  --from-file=uvicorn_app.py=uvicorn_app.py

# Apply config
kubectl apply -f k8s/configmap.yaml
```

### 5. Deploy

```bash
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
```

### 6. Wait for Rollout

```bash
kubectl rollout status deployment/neutrino
```

## Configuration

### Customize Worker Count

Edit `k8s/configmap.yaml`:

```yaml
data:
  config.yaml: |
    orchestrator:
      worker_count: 8  # Increase workers per pod
```

### Scale Pods

```bash
# Scale to 4 pods
kubectl scale deployment/neutrino --replicas=4
```

### Change App Module

To deploy a different Neutrino app:

```bash
# Set environment variable
export APP_MODULE=myapp.main

# Run deploy script
./scripts/deploy.sh
```

Or edit `k8s/configmap.yaml`:

```yaml
data:
  config.yaml: |
    orchestrator:
      app_module: "myapp.main"
```

## Testing Endpoints

### Health Check

```bash
curl http://localhost:8080/health
```

### FastAPI Routes (ASGI fallback)

```bash
# List users
curl http://localhost:8080/api/users

# Get specific user
curl http://localhost:8080/users/1
```

### Neutrino Routes (Orchestrator)

```bash
# Heavy processing task
curl -X POST http://localhost:8080/neutrino/process \
  -H "Content-Type: application/json" \
  -d '{"text":"hello","iterations":1000}'

# Analysis task
curl -X POST http://localhost:8080/neutrino/analyze \
  -H "Content-Type: application/json" \
  -d '{"user_id":1,"data":[1.5,2.3,3.7,4.2,5.8]}'
```

## Monitoring

### View Logs

```bash
# All pods
kubectl logs -f deployment/neutrino

# Specific pod
kubectl logs -f neutrino-<pod-id>

# Previous container (if crashed)
kubectl logs neutrino-<pod-id> --previous
```

### Check Resource Usage

```bash
kubectl top pods -l app=neutrino
```

### Debug Pod

```bash
# Get shell in pod
kubectl exec -it deployment/neutrino -- /bin/bash

# Check worker processes
kubectl exec deployment/neutrino -- ps aux | grep python

# Check config
kubectl exec deployment/neutrino -- cat /app/config.yaml
```

## Troubleshooting

### Pods Not Starting

```bash
# Describe pod to see events
kubectl describe pod neutrino-<pod-id>

# Check image pull
kubectl get events | grep neutrino
```

### Image Pull Errors

If you see `ImagePullBackOff`:

```bash
# Verify image exists in k3s
sudo k3s crictl images | grep neutrino

# Re-import image
docker save neutrino:latest -o /tmp/neutrino.tar
sudo k3s ctr images import /tmp/neutrino.tar
```

### Health Check Failing

```bash
# Check if Rust binary is running
kubectl exec deployment/neutrino -- ps aux | grep neutrino

# Check logs for errors
kubectl logs deployment/neutrino | grep -i error

# Test health endpoint inside pod
kubectl exec deployment/neutrino -- curl localhost:8080/health
```

### Workers Not Spawning

```bash
# Check Python is available
kubectl exec deployment/neutrino -- which python3

# Check PYTHONPATH
kubectl exec deployment/neutrino -- env | grep PYTHON

# Verify neutrino module exists
kubectl exec deployment/neutrino -- ls -la /app/python/neutrino
```

## Cleanup

```bash
# Delete all resources
kubectl delete -f k8s/

# Delete ConfigMap for app code
kubectl delete configmap neutrino-app-code

# Remove Docker image
docker rmi neutrino:latest
```

## Production Considerations

### Resource Limits

Adjust resources in `deployment.yaml` based on workload:

```yaml
resources:
  requests:
    memory: "1Gi"    # Minimum guaranteed
    cpu: "1000m"
  limits:
    memory: "4Gi"    # Maximum allowed
    cpu: "4000m"
```

### Autoscaling

Add HorizontalPodAutoscaler:

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: neutrino-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: neutrino
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
```

### Persistent Storage (for logs/state)

Add PersistentVolumeClaim if needed:

```yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: neutrino-storage
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
```

Mount in deployment:

```yaml
volumeMounts:
- name: storage
  mountPath: /app/data
volumes:
- name: storage
  persistentVolumeClaim:
    claimName: neutrino-storage
```

## Next Steps

- Add Ingress for external access
- Set up monitoring (Prometheus/Grafana)
- Configure autoscaling based on queue depth
- Add separate deployments for model serving (Phase 2)
