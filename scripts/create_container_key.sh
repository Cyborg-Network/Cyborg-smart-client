#!/bin/bash

error() {
    echo -e "\033[31m$1\033[0m" >&2
    exit 1
}

CONTAINER="$1"
[ -z "$CONTAINER" ] && error "Usage: $0 <container>"

if [[ ! -x "$(command -v docker)" ]]; then 
    error "Docker is not installed"
fi

if [[ "$(docker inspect -f '{{.State.Running}}' $CONTAINER)" != "true" ]]; then
    error "Container $CONTAINER is not running"
fi

TMP_KEY=$(mktemp -p /dev/shm)
TMP_KEY_PUB="${TMP_KEY}.pub"

ssh-keygen -t ed25519 -f "$TMP_KEY" -N "" -q || error "Failed to generate keypair"

PRIVATE_KEY=$(cat "$TMP_KEY")
PUBLIC_KEY=$(cat "$TMP_KEY_PUB")

docker exec "$CONTAINER" mkdir -p /root/.ssh
docker exec "$CONTAINER" chmod 700 /root/.ssh
docker exec "$CONTAINER" sh -c "echo '$PUBLIC_KEY' >> /root/.ssh/authorized_keys"
docker exec "$CONTAINER" chmod 600 /root/.ssh/authorized_keys

jq -n --arg priv "$PRIVATE_KEY" --arg pub "$PUBLIC_KEY" '{private_key: $priv, public_key: $pub}'

rm -f "$TMP_KEY" "$TMP_KEY_PUB"
