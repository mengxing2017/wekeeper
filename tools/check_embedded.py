"""POSIX lifecycle smoke test for the real embedded-PG development entry."""

import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request


REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / "backend/target/debug/examples/embedded_postgres"
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def port_is_open(port):
    with socket.socket() as connection:
        connection.settimeout(1)
        return connection.connect_ex(("127.0.0.1", port)) == 0


def assert_database_cleaned(log):
    started = []
    for line in log.splitlines():
        try:
            fields = json.loads(line).get("fields", {})
        except ValueError:
            continue
        if fields.get("message") == "embedded PostgreSQL started; data will be removed on exit":
            started.append(fields)
    assert len(started) == 1, f"missing database startup evidence:\n{log}"
    database = started[0]
    assert not Path(database["data_directory"]).exists(), "temporary data was not removed"
    assert not port_is_open(database["database_port"]), "embedded database still accepts connections"


def check_case(shutdown_signal=None):
    # Keep the socket occupied for the failure case; release it for normal runs.
    with socket.socket() as reserved:
        reserved.bind(("127.0.0.1", 0))
        port = reserved.getsockname()[1]
        reserved.listen()
        if shutdown_signal is not None:
            reserved.close()
        env = {
            **os.environ,
            "WEKEEPER_BIND_ADDR": f"127.0.0.1:{port}",
            "RUST_LOG": "wekeeper_api=info,embedded_postgres=info",
            # The example must ignore all external database connection settings.
            "WEKEEPER_DATABASE_URL": "not-a-postgres-url",
            "WEKEEPER_DATABASE_HOST": "invalid.example",
            "WEKEEPER_DATABASE_PORT": "not-a-port",
            "WEKEEPER_DATABASE_PASSWORD": "unused-test-value",
        }
        for key in ("WEKEEPER_DEV_ADMIN_PASSWORD", "WEKEEPER_INSECURE_LOCAL_AUTH", "WEKEEPER_PUBLIC_ORIGIN"):
            env.pop(key, None)
        custom_password = "EmbeddedTestPassword123!" if shutdown_signal == signal.SIGTERM else None
        if custom_password:
            env["WEKEEPER_DEV_ADMIN_PASSWORD"] = custom_password
        with tempfile.TemporaryFile(mode="w+") as output:
            proc = subprocess.Popen([str(BINARY)], cwd=REPO, env=env, stdout=output, stderr=subprocess.STDOUT)
            try:
                if shutdown_signal is None:
                    assert proc.wait(timeout=120) != 0, "occupied HTTP port should fail startup"
                else:
                    deadline = time.monotonic() + 120
                    while time.monotonic() < deadline:
                        if proc.poll() is not None:
                            output.seek(0)
                            raise AssertionError(f"development entry exited early:\n{output.read()}")
                        try:
                            with HTTP.open(f"http://127.0.0.1:{port}/readyz", timeout=1) as response:
                                if json.load(response) == {"status": "ready"}:
                                    break
                        except OSError:
                            pass
                        time.sleep(0.2)
                    else:
                        raise AssertionError("embedded API did not become ready")
                    with HTTP.open(f"http://127.0.0.1:{port}/api/v1/system", timeout=2) as response:
                        assert json.load(response)["data"]["attributes"]["application"] == "wekeeper"
                    output.seek(0)
                    startup_log = output.read()
                    if custom_password:
                        assert custom_password not in startup_log, "custom password leaked to output"
                        password = custom_password
                    else:
                        password = next(line.split("：", 1)[1] for line in startup_log.splitlines()
                                        if line.startswith("本次临时管理员密码："))
                        assert len(password) == 32
                    request = urllib.request.Request(
                        f"http://127.0.0.1:{port}/api/v1/auth/sessions",
                        data=json.dumps({"data":{"type":"sessions","attributes":{"username": "admin", "password": password}}}).encode(),
                        headers={"Content-Type": "application/vnd.api+json", "Origin": "http://127.0.0.1:5173"},
                    )
                    with HTTP.open(request, timeout=5) as response:
                        assert json.load(response)["data"]["attributes"]["user"]["platform_admin"] is True
                        cookie = response.headers["Set-Cookie"]
                        assert "HttpOnly" in cookie and "; Secure" not in cookie
                    proc.send_signal(shutdown_signal)
                    assert proc.wait(timeout=20) == 0, "shutdown was not graceful"
                output.seek(0)
                assert_database_cleaned(output.read())
            finally:
                if proc.poll() is None:
                    proc.send_signal(signal.SIGINT)
                    try:
                        proc.wait(timeout=20)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()


if __name__ == "__main__":
    if os.name != "posix":
        raise SystemExit("This signal lifecycle check requires POSIX; use Ctrl+C manually on Windows.")
    for stop_signal in (signal.SIGINT, signal.SIGTERM):
        check_case(stop_signal)
        print(f"{stop_signal.name}: admin login, migration/readiness, isolated config, database stop/data cleanup passed")
    check_case()
    print("HTTP port conflict: error exit and database stop/data cleanup passed")
