FROM registry.opensuse.org/opensuse/leap:16.0
RUN zypper -n --gpg-auto-import-keys in --no-recommends cargo rust gcc git-core && zypper clean -a
