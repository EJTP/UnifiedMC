#!/bin/sh
# One class, one dependency. Gradle here would be ceremony.
set -e
cd "$(dirname "$0")"
JDK=../.toolchain/jdk21/bin
rm -rf out && mkdir -p out
"$JDK/javac" -cp ../.toolchain/fml.jar -d out --release 21 src/dev/unifiedmc/server/*.java
"$JDK/java" -ea -cp out dev.unifiedmc.server.UnifiedMcServer
"$JDK/jar" --create --file unifiedmc-server-0.1.0.jar -C out . -C res .
echo "built $(pwd)/unifiedmc-server-0.1.0.jar"
