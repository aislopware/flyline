# flyline publishes Android builds for aarch64 only, so test the aarch64 Termux image.
FROM termux/termux-docker:aarch64

ARG FLYLINE_INSTALL_VERSION

RUN script -q -e -c "/data/data/com.termux/files/usr/bin/bash -i -c 'FLYLINE_INSTALL_VERSION=${FLYLINE_INSTALL_VERSION} source <(curl -sSfL https://github.com/aislopware/flyline/releases/download/${FLYLINE_INSTALL_VERSION}/install.sh)'" /dev/null

RUN script -q -e -c "/data/data/com.termux/files/usr/bin/bash -i -c 'flyline --version'" /dev/null
