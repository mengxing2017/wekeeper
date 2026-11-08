"""Supply ephemeral credentials only for the fresh embedded development database."""

import os
import secrets
import sys


SOURCE_MASTER_KEY = "WEKEEPER_SOURCE_MASTER_KEY_V1"


def prepare_environment(environment):
    child = dict(environment)
    generated = not child.get(SOURCE_MASTER_KEY)
    if generated:
        child[SOURCE_MASTER_KEY] = secrets.token_urlsafe(32)
    return child, generated


def main():
    environment, generated = prepare_environment(os.environ)
    if generated:
        print("已为本次临时开发数据库自动配置凭据加密密钥（不输出密钥值）。", file=sys.stderr, flush=True)
    command = ["cargo", "run", "--locked", "--manifest-path", "backend/Cargo.toml", "--example", "embedded_postgres"]
    # Replace the launcher so Ctrl+C/SIGTERM retain the existing Cargo lifecycle.
    os.execvpe(command[0], command, environment)


if __name__ == "__main__":
    main()
