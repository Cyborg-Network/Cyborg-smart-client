#!/bin/bash

set -e

trap 'echo "An error occurred during installation, please check the logs for further information."; exit -1' ERR

HTTP_PORT=8080
WS_PORT=8081

BINARY_URL="tombleek.dev/downloads/cyborg-agent"
BINARY_NAME="cyborg-agent"
BINARY_PATH="/usr/local/bin/$BINARY_NAME"
SERVICE_FILE="/etc/systemd/system/$BINARY_NAME.service"

echo "Downloading the binary from $BINARY_URL..."
curl -L -o $BINARY_NAME $BINARY_URL

chmod +x $BINARY_NAME

echo "Moving the binary to /usr/local/bin..."
sudo mv $BINARY_NAME $BINARY_PATH

echo "Creating systemd service..."
sudo bash -c "cat > $SERVICE_FILE" << EOL
[Unit]
Description=Agent that is able to check the health of the node, provide reuired info to the cyborg-parachain, and stream usage metrics and logs of the cyborg node.
After=network.target

[Service]
ExecStart=$BINARY_PATH run
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
EOL

echo "Reloading systemd, enabling and starting $BINARY_NAME service..."
sudo systemctl daemon-reload
sudo systemctl enable $BINARY_NAME
sudo systemctl start $BINARY_NAME

sudo systemctl status $BINARY_NAME --no-pager

echo "Cyborg Agent is installed and running. Binary located at $BINARY_PATH. Now attempting to open Port $HTTP_PORT and $WS_PORT."

if command -v ufw &> /dev/null; then
    FIREWALL="ufw"
elif command -v firewall-cmd &> /dev/null; then
    FIREWALL="firewalld"
elif command -v iptables &> /dev/null; then
    FIREWALL="iptables"
else
    echo "Firewall management tool not detected. Please open $HTTP_PORT and $WS_PORT manually for the agent to work."
    echo "If in doubt, refer to the documentation of your firewall management tool for instructions."
fi

open_ports_ufw() {
    sudo ufw allow $WS_PORT
    sudo ufw allow $HTTP_PORT
    echo "Ports opened in UFW."
}

# Function to open ports with firewalld
open_ports_firewalld() {
    sudo firewall-cmd --permanent --add-port=$HTTP_PORT/tcp
    sudo firewall-cmd --permanent --add-port=$WS_PORT/tcp
    sudo firewall-cmd --reload
    echo "Ports opened in firewalld."
}

# Function to open ports with iptables
open_ports_iptables() {
    sudo iptables -A INPUT -p tcp --dport $HTTP_PORT -j ACCEPT
    sudo iptables -A INPUT -p tcp --dport $WS_PORT -j ACCEPT
    # Note: Rules added with iptables are not persistent across reboots unless saved.
    echo "Ports opened in iptables."
}

case $FIREWALL in
    "ufw")
        open_ports_ufw
        ;;
    "firewalld")
        open_ports_firewalld
        ;;
    "iptables")
        open_ports_iptables
        ;;
esac
