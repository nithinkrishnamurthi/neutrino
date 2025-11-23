#!/bin/bash
set -e

# Neutrino k3s Deployment Script
# This script:
# 1. Generates OpenAPI spec and Uvicorn startup script
# 2. Builds Docker image
# 3. Imports image to k3s
# 4. Deploys to k3s cluster

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
APP_MODULE="${APP_MODULE:-examples.fastapi_integration}"
IMAGE_NAME="${IMAGE_NAME:-neutrino:latest}"
NAMESPACE="${NAMESPACE:-default}"

echo -e "${GREEN}=== Neutrino k3s Deployment ===${NC}"
echo ""

# Check prerequisites
echo -e "${YELLOW}[1/6] Checking prerequisites...${NC}"

if ! command -v kubectl &> /dev/null; then
    echo -e "${RED}Error: kubectl not found. Please install kubectl.${NC}"
    exit 1
fi

if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: docker not found. Please install docker.${NC}"
    exit 1
fi

# Setup kubeconfig for k3s if needed
if ! kubectl cluster-info &> /dev/null; then
    echo -e "${YELLOW}kubectl not configured. Setting up kubeconfig for k3s...${NC}"

    # Check if k3s is installed and running
    if ! systemctl is-active --quiet k3s; then
        echo -e "${RED}Error: k3s service is not running. Start it with: sudo systemctl start k3s${NC}"
        exit 1
    fi

    # Copy k3s config to user's kubeconfig
    if [ -f "/etc/rancher/k3s/k3s.yaml" ]; then
        mkdir -p ~/.kube
        sudo cp /etc/rancher/k3s/k3s.yaml ~/.kube/config
        sudo chown $USER:$USER ~/.kube/config
        chmod 600 ~/.kube/config
        export KUBECONFIG=~/.kube/config
        echo -e "${GREEN}✓ Kubeconfig configured${NC}"

        # Verify connection
        if ! kubectl cluster-info &> /dev/null; then
            echo -e "${RED}Error: Still cannot connect to k3s. Try: sudo k3s kubectl get nodes${NC}"
            exit 1
        fi
    else
        echo -e "${RED}Error: k3s config not found at /etc/rancher/k3s/k3s.yaml${NC}"
        exit 1
    fi
fi

echo -e "${GREEN}✓ Prerequisites check passed${NC}"
echo ""

# Generate OpenAPI spec and Uvicorn script
echo -e "${YELLOW}[2/6] Generating OpenAPI spec and Uvicorn script...${NC}"

# Change to project root
cd "$(dirname "$0")/.."

# Activate virtual environment if it exists
if [ -d ".venv" ]; then
    source .venv/bin/activate
elif [ -d "python/.venv" ]; then
    source python/.venv/bin/activate
fi

# Run neutrino deploy command
python -m cli.main deploy "$APP_MODULE" --openapi

if [ ! -f "openapi.json" ]; then
    echo -e "${RED}Error: Failed to generate openapi.json${NC}"
    exit 1
fi

if [ ! -f "uvicorn_app.py" ]; then
    echo -e "${RED}Error: Failed to generate uvicorn_app.py${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Generated openapi.json and uvicorn_app.py${NC}"
echo ""

# Build Docker image
echo -e "${YELLOW}[3/6] Building Docker image...${NC}"
docker build -t "$IMAGE_NAME" .

echo -e "${GREEN}✓ Docker image built: $IMAGE_NAME${NC}"
echo ""

# Import image to k3s
echo -e "${YELLOW}[4/6] Importing image to k3s...${NC}"

# Save Docker image to tar
docker save "$IMAGE_NAME" -o /tmp/neutrino-image.tar

# Import to k3s
if command -v k3s &> /dev/null; then
    sudo k3s ctr images import /tmp/neutrino-image.tar
else
    # If k3s command not available, try importing via crictl
    if command -v crictl &> /dev/null; then
        sudo crictl pull "docker-archive:///tmp/neutrino-image.tar"
    else
        echo -e "${YELLOW}Warning: Could not import to k3s. Image may need to be pulled from registry.${NC}"
    fi
fi

# Clean up tar file
rm -f /tmp/neutrino-image.tar

echo -e "${GREEN}✓ Image imported to k3s${NC}"
echo ""

# Create ConfigMap for app code
echo -e "${YELLOW}[5/6] Creating Kubernetes resources...${NC}"

# Create ConfigMap from generated files
kubectl create configmap neutrino-app-code \
    --from-file=openapi.json=openapi.json \
    --from-file=uvicorn_app.py=uvicorn_app.py \
    --namespace="$NAMESPACE" \
    --dry-run=client -o yaml | kubectl apply -f -

# Apply all k8s manifests
kubectl apply -f k8s/configmap.yaml --namespace="$NAMESPACE"
kubectl apply -f k8s/deployment.yaml --namespace="$NAMESPACE"
kubectl apply -f k8s/service.yaml --namespace="$NAMESPACE"

echo -e "${GREEN}✓ Kubernetes resources created${NC}"
echo ""

# Wait for deployment
echo -e "${YELLOW}[6/6] Waiting for deployment to be ready...${NC}"
kubectl rollout status deployment/neutrino --namespace="$NAMESPACE" --timeout=120s

echo -e "${GREEN}✓ Deployment ready${NC}"
echo ""

# Get service endpoint
echo -e "${GREEN}=== Deployment Complete ===${NC}"
echo ""
echo "Service endpoints:"
kubectl get service neutrino --namespace="$NAMESPACE"
echo ""

# Get LoadBalancer IP/hostname
EXTERNAL_IP=$(kubectl get service neutrino --namespace="$NAMESPACE" -o jsonpath='{.status.loadBalancer.ingress[0].ip}')
if [ -z "$EXTERNAL_IP" ]; then
    EXTERNAL_IP=$(kubectl get service neutrino --namespace="$NAMESPACE" -o jsonpath='{.status.loadBalancer.ingress[0].hostname}')
fi

if [ -z "$EXTERNAL_IP" ]; then
    echo -e "${YELLOW}LoadBalancer external IP pending. For local k3s, try:${NC}"
    echo "  http://localhost:8080"
else
    echo -e "${GREEN}Neutrino is available at:${NC}"
    echo "  http://$EXTERNAL_IP:8080"
fi

echo ""
echo "Test endpoints:"
echo "  Health check:  curl http://localhost:8080/health"
echo "  FastAPI users: curl http://localhost:8080/api/users"
echo "  Neutrino task: curl -X POST http://localhost:8080/neutrino/process -H 'Content-Type: application/json' -d '{\"text\":\"hello\",\"iterations\":100}'"
echo ""
echo "View logs:"
echo "  kubectl logs -f deployment/neutrino --namespace=$NAMESPACE"
echo ""
echo "View pods:"
echo "  kubectl get pods -l app=neutrino --namespace=$NAMESPACE"
