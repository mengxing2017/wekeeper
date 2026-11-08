"""Check key isolation and actual environment delivery without opening sockets."""

import base64
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from dev_embedded import SOURCE_MASTER_KEY, prepare_environment


class EmbeddedEnvironmentTests(unittest.TestCase):
    def test_missing_or_empty_key_is_fresh_and_does_not_modify_parent(self):
        for original in ({"OTHER": "kept"}, {SOURCE_MASTER_KEY: "", "OTHER": "kept"}):
            before = dict(original)
            first, generated = prepare_environment(original)
            second, _ = prepare_environment(original)
            self.assertTrue(generated)
            self.assertEqual(len(base64.urlsafe_b64decode(first[SOURCE_MASTER_KEY] + "=")), 32)
            self.assertNotIn("=", first[SOURCE_MASTER_KEY])
            self.assertNotEqual(first[SOURCE_MASTER_KEY], second[SOURCE_MASTER_KEY])
            self.assertEqual(first["OTHER"], "kept")
            self.assertEqual(original, before)

    def test_explicit_key_is_preserved_even_when_invalid(self):
        # Do not silently replace a supplied key and hide a configuration mistake.
        for key in ("user-configured-value", " "):
            environment, generated = prepare_environment({SOURCE_MASTER_KEY: key})
            self.assertFalse(generated)
            self.assertEqual(environment[SOURCE_MASTER_KEY], key)

    def test_launcher_passes_key_only_in_child_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            cargo = Path(directory) / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import base64,os,sys\n"
                f"value=os.environ[{SOURCE_MASTER_KEY!r}]\n"
                "assert len(base64.urlsafe_b64decode(value+'='))==32\n"
                "assert value not in ' '.join(sys.argv)\n"
                "assert sys.argv[1:]==['run','--locked','--manifest-path','backend/Cargo.toml','--example','embedded_postgres']\n"
                "print('embedded-environment-ok')\n"
            )
            cargo.chmod(0o700)
            environment = {**os.environ, "PATH": directory}
            environment.pop(SOURCE_MASTER_KEY, None)
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("dev_embedded.py"))],
                env=environment, capture_output=True, text=True, check=True,
            )
            self.assertEqual(result.stdout, "embedded-environment-ok\n")
            self.assertIn("自动配置", result.stderr)
            self.assertIn("不输出密钥值", result.stderr)


if __name__ == "__main__":
    unittest.main()
