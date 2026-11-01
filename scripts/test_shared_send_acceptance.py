"""Isolated synthetic acceptance-validator tests. Never publishes or sends."""
import importlib.util
import tempfile
import unittest
import hashlib
from datetime import datetime, timedelta, timezone
from pathlib import Path
from uuid import uuid4
from zipfile import ZipFile

spec = importlib.util.spec_from_file_location('acceptance', Path(__file__).with_name('shared-send-acceptance.py'))
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class AcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='shared-acceptance-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        apk = self.root / 'synthetic-test-only.apk'
        with ZipFile(apk, 'w') as out:
            out.writestr('AndroidManifest.xml', 'synthetic')
            out.writestr('classes.dex', 'synthetic')
        evidence = self.root / 'synthetic.txt'
        evidence.write_text('Unit test fixture; not actual device acceptance')
        artifact = lambda p: {'path': p.name, 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
        self.m = {key: str(uuid4()) for key in ['device_id', 'school_id', 'target_id', 'release_id', 'environment_snapshot_id', 'reviewer_id']}
        self.m.update(stage='test_send', preflight_passed=True, actual_apk=artifact(apk), artifacts=[artifact(evidence)], expires_at=(datetime.now(timezone.utc) + timedelta(hours=1)).isoformat())

    def test_test_scope_has_no_database_side_effect(self):
        m = acceptance.validate(self.m, self.root)
        sql = acceptance.render(m)
        self.assertIn('mobile_shared_acceptance_runs', sql)
        self.assertIn('mobile_shared_release_scopes', sql)
        self.assertNotIn('production_allowlist', sql)

    def test_changed_artifact_is_rejected(self):
        (self.root / 'synthetic.txt').write_text('changed')
        with self.assertRaises(ValueError):
            acceptance.validate(self.m, self.root)

    def test_production_requires_soak_all_cases_and_linked_evidence(self):
        self.m.update(stage='production_allowlist')
        with self.assertRaises(ValueError):
            acceptance.validate(self.m, self.root)
        self.m.update(soak_hours=72, soak_passed=True, historical_debt_closed=True, cases={f'S{i:02}': 'passed' for i in range(1, 41)})
        with self.assertRaises(ValueError):
            acceptance.validate(self.m, self.root)
        self.m['case_evidence'] = {k: [self.m['artifacts'][0]['sha256']] for k in self.m['cases']}
        acceptance.validate(self.m, self.root)
        self.m['cases']['S40'] = 'failed'
        with self.assertRaises(ValueError):
            acceptance.validate(self.m, self.root)

    def test_bundle_cannot_read_outside_artifacts(self):
        self.m['artifacts'][0]['path'] = '../outside'
        with self.assertRaises(ValueError):
            acceptance.validate(self.m, self.root)


if __name__ == '__main__':
    unittest.main()
