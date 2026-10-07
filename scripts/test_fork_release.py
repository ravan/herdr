import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from scripts import fork_release


class ForkReleaseTests(unittest.TestCase):
    def test_tag_identity_and_rejected_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            cargo = Path(temporary) / "Cargo.toml"
            cargo.write_text('[package]\nversion = "0.9.3"\n')
            metadata = fork_release.prepare("houston-v0.9.3-2", cargo, "a" * 40)
            self.assertEqual(metadata["version"], "0.9.3-houston.2")
            self.assertEqual(metadata["namespace"], "herdr-houston")
            self.assertEqual(metadata["tagline"], "Houston, we have a pane")
            for tag in ("v0.9.3", "preview-test", "houston-v0.9.4-1", "houston-v0.9.3-0", "houston-v0.9.3-01", "houston-v0.9.3-1\nunsafe=x", "houston-v0.9.3-$(echo bad)"):
                with self.subTest(tag=tag), self.assertRaises(ValueError):
                    fork_release.prepare(tag, cargo, "a" * 40)

    def test_bundle_verifies_five_assets_provenance_and_checksums_before_copying(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cargo = root / "Cargo.toml"
            cargo.write_text('[package]\nversion = "0.9.3"\n')
            metadata = fork_release.prepare("houston-v0.9.3-1", cargo, "a" * 40)
            artifacts = root / "artifacts"
            for name in fork_release.ASSETS:
                directory = artifacts / name
                directory.mkdir(parents=True)
                payload = name.encode()
                (directory / name).write_bytes(payload)
                (directory / f"{name}.build.json").write_text(json.dumps(metadata))
                digest = hashlib.sha256(payload).hexdigest()
                (directory / f"{name}.sha256").write_text(f"{digest}  {name}\n")
            bad = artifacts / fork_release.ASSETS[0] / fork_release.ASSETS[0]
            bad.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "checksum"):
                fork_release.bundle(artifacts, root / "rejected", metadata)
            self.assertFalse((root / "rejected").exists())
            bad.write_bytes(bad.name.encode())
            provenance = bad.with_name(bad.name + ".build.json")
            provenance.write_text(json.dumps({**metadata, "commit": "b" * 40}))
            with self.assertRaisesRegex(ValueError, "provenance"):
                fork_release.bundle(artifacts, root / "wrong-source", metadata)
            self.assertFalse((root / "wrong-source").exists())
            provenance.write_text(json.dumps(metadata))
            fork_release.bundle(artifacts, root / "release", metadata)
            self.assertEqual(len((root / "release" / "SHA256SUMS").read_text().splitlines()), 5)
            self.assertEqual(set(json.loads((root / "release" / "FORK_BUILD.json").read_text())["sha256"]), set(fork_release.ASSETS))
            self.assertEqual((root / "release" / bad.name).read_bytes(), bad.read_bytes())
