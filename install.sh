#!/bin/sh
set -e

REPO="myersm0/transparent-poker"

curl --proto '=https' --tlsv1.2 -LsSf \
    "https://github.com/${REPO}/releases/latest/download/transparent-poker-installer.sh" | sh
