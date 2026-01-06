#!/bin/sh
set -e

REPO="myersm0/poker-terminal"

curl --proto '=https' --tlsv1.2 -LsSf \
    "https://github.com/${REPO}/releases/latest/download/poker-terminal-installer.sh" | sh
