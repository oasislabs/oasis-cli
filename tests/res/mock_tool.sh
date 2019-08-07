#!/bin/bash
echo "BEGIN MOCK $0"
env
echo "---"
for arg in "$@"; do
  echo "$arg"
done
echo "---"
$user_script
echo "END MOCK $0"
