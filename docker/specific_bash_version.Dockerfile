# Stays on 24.04 (GCC 13): 26.04's GCC 15 defaults to C23, where the K&R
# declarations in bash 3.2/4.4 sources are errors (mkbuiltins.c: "too many
# arguments to function 'xmalloc'").
FROM ubuntu:24.04

ARG DOCKER_BASH_VERSION

RUN apt update && apt install -y \
  build-essential \
  ca-certificates \
  curl \
  libreadline-dev \
  libncurses-dev \
  bison \
  yacc \
  util-linux \
  python3 \
  autotools-dev \
  && rm -rf /var/lib/apt/lists/*


WORKDIR /tmp/bash-build
RUN curl -LO https://ftp.gnu.org/gnu/bash/bash-${DOCKER_BASH_VERSION}.tar.gz \
 && tar xzf bash-${DOCKER_BASH_VERSION}.tar.gz \
 && cd bash-${DOCKER_BASH_VERSION} \
 && cp -f /usr/share/misc/config.guess support/config.guess 2>/dev/null || true \
 && cp -f /usr/share/misc/config.sub support/config.sub 2>/dev/null || true \
 && ./configure --prefix=/opt/bash-${DOCKER_BASH_VERSION} --with-readline \
 && make -j"$(nproc)" \
 && make install \
 && cd .. \
 && rm -rf bash-${DOCKER_BASH_VERSION}* 

RUN rm /bin/bash && ln -s /opt/bash-${DOCKER_BASH_VERSION}/bin/bash /bin/bash
