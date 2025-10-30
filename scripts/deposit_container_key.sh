#!/bin/bash

error() {
    echo -e "\033[31m$1\033[0m" >&2
    exit 1
}

CONTAINER="$1"
[ -z "$CONTAINER" ] && error "Usage: $0 <container> <public key>"

PUB_KEY="$2"
[ -z "$PUB_KEY" ] && error "Usage: $0 <container> <public key>"

PUB_KEY="$(echo "$PUB_KEY" | xargs)"

if ! [[ "$PUB_KEY" =~ ^(ssh-ed25519|ssh-rsa|ecdsa-sha2-nistp256) ]]; then
    error "Invalid public key format"
fi

if [[ ! -x "$(command -v docker)" ]]; then 
    error "Docker is not installed"
fi

if [[ "$(docker inspect -f '{{.State.Running}}' $CONTAINER)" != "true" ]]; then
    error "Container $CONTAINER is not running"
fi

docker exec "$CONTAINER" mkdir -p /root/.ssh
docker exec "$CONTAINER" chmod 700 /root/.ssh
echo "$PUB_KEY" | docker exec -i "$CONTAINER" sh -c "cat >> /root/.ssh/authorized_keys"
docker exec "$CONTAINER" chmod 600 /root/.ssh/authorized_keys
