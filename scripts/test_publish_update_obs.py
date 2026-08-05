import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock


MODULE_PATH = pathlib.Path(__file__).with_name("publish-update-obs.py")
SPEC = importlib.util.spec_from_file_location("publish_update_obs", MODULE_PATH)
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


class ArchDetectionTests(unittest.TestCase):
    def test_arch_from_path_reads_the_target_triple(self):
        p = pathlib.Path("/repo/target/x86_64-apple-darwin/release/bundle/macos/Memento.app.tar.gz")
        self.assertEqual(publisher.arch_from_path(p), "x86_64")
        p = pathlib.Path("/repo/target/aarch64-apple-darwin/release/bundle/macos/Memento.app.tar.gz")
        self.assertEqual(publisher.arch_from_path(p), "aarch64")

    def test_arch_from_path_returns_none_without_a_triple(self):
        p = pathlib.Path("/repo/target/release/bundle/macos/Memento.app.tar.gz")
        self.assertIsNone(publisher.arch_from_path(p))

    def test_arch_from_name_prefers_aarch64_over_the_x86_substring(self):
        # "x86_64" contains "x86"; an arm64 name must not fall through to it.
        self.assertEqual(publisher.arch_from_name("Memento_0.4.0_arm64-setup.exe"), "aarch64")
        self.assertEqual(publisher.arch_from_name("Memento_0.4.0_aarch64.dmg"), "aarch64")

    def test_arch_from_name_maps_the_x86_aliases(self):
        for name in ("Memento_0.4.0_x64-setup.exe", "app_amd64.AppImage", "Memento-intel.dmg"):
            self.assertEqual(publisher.arch_from_name(name), "x86_64", name)

    def test_arch_from_name_returns_none_when_absent(self):
        self.assertIsNone(publisher.arch_from_name("Memento.app.tar.gz"))


class PlatformKeyTests(unittest.TestCase):
    def test_target_override_wins(self):
        key, how = publisher.platform_key_for(pathlib.Path("Memento.app.tar.gz"), "darwin-x86_64")
        self.assertEqual((key, how), ("darwin-x86_64", "--target"))

    def test_app_tarball_uses_the_target_triple_in_the_path(self):
        p = pathlib.Path("/repo/target/x86_64-apple-darwin/release/bundle/macos/Memento.app.tar.gz")
        key, how = publisher.platform_key_for(p, None)
        self.assertEqual(key, "darwin-x86_64")
        self.assertEqual(how, "target triple in path")

    def test_ambiguous_app_tarball_is_refused_rather_than_guessed(self):
        """The P1 from review: guessing the host arch can serve an incompatible binary."""
        with tempfile.TemporaryDirectory() as tmp:
            payload = pathlib.Path(tmp) / "Memento.app.tar.gz"
            payload.write_bytes(b"x")  # no sibling .app, no triple in the path
            key, how = publisher.platform_key_for(payload, None)
        self.assertIsNone(key)
        self.assertIn("sibling .app is missing", how)

    def test_app_tarball_falls_back_to_the_sibling_app_arch(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            binary = root / "Memento.app" / "Contents" / "MacOS" / "Memento"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"macho")
            payload = root / "Memento.app.tar.gz"
            payload.write_bytes(b"x")
            with mock.patch.object(publisher.subprocess, "run") as run:
                run.return_value = mock.Mock(stdout="arm64\n")
                key, how = publisher.platform_key_for(payload, None)
        self.assertEqual(key, "darwin-aarch64")
        self.assertEqual(how, "arch of the sibling .app binary")

    def test_universal_sibling_app_is_ambiguous(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            binary = root / "Memento.app" / "Contents" / "MacOS" / "Memento"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"macho")
            payload = root / "Memento.app.tar.gz"
            payload.write_bytes(b"x")
            with mock.patch.object(publisher.subprocess, "run") as run:
                run.return_value = mock.Mock(stdout="x86_64 arm64\n")
                key, _ = publisher.platform_key_for(payload, None)
        self.assertIsNone(key)

    def test_windows_and_linux_artifacts_come_from_the_filename(self):
        cases = {
            "Memento_0.4.0_x64-setup.exe": "windows-x86_64",
            "Memento_0.4.0_x64_en-US.msi": "windows-x86_64",
            "Memento_0.4.0_arm64-setup.exe": "windows-aarch64",
            "memento_0.4.0_amd64.AppImage": "linux-x86_64",
            "memento_0.4.0_aarch64.deb": "linux-aarch64",
            "Memento_0.4.0_aarch64.dmg": "darwin-aarch64",
        }
        for name, expected in cases.items():
            key, _ = publisher.platform_key_for(pathlib.Path(f"/dist/{name}"), None)
            self.assertEqual(key, expected, name)

    def test_every_detected_key_is_a_supported_platform(self):
        for name in ("Memento_0.4.0_x64-setup.exe", "Memento_0.4.0_aarch64.dmg"):
            key, _ = publisher.platform_key_for(pathlib.Path(f"/dist/{name}"), None)
            self.assertIn(key, publisher.VALID_PLATFORMS, name)


class PayloadPreferenceTests(unittest.TestCase):
    def test_nsis_outranks_msi(self):
        nsis = pathlib.Path("/dist/Memento_0.4.0_x64-setup.exe")
        msi = pathlib.Path("/dist/Memento_0.4.0_x64_en-US.msi")
        self.assertLess(publisher.payload_rank(nsis), publisher.payload_rank(msi))

    def test_equivalent_payloads_share_a_rank(self):
        a = pathlib.Path("/a/Memento.app.tar.gz")
        b = pathlib.Path("/b/Memento.app.tar.gz")
        self.assertEqual(publisher.payload_rank(a), publisher.payload_rank(b))


class SemverTests(unittest.TestCase):
    def test_accepts_plain_and_prerelease_versions(self):
        for version in ("0.4.0", "1.0.0", "0.4.1-beta.1", "0.4.1+build.5"):
            self.assertTrue(publisher.is_semver(version), version)

    def test_rejects_the_four_component_scheme_from_release_yml(self):
        # release.yml appends a fourth component when a tag exists; Tauri can't parse it.
        for version in ("0.4.0.1", "0.4", "v0.4.0", "latest", ""):
            self.assertFalse(publisher.is_semver(version), version)


class StartManifestTests(unittest.TestCase):
    def remote(self):
        return {
            "version": "0.4.0",
            "notes": "remote notes",
            "pub_date": "2026-08-05T10:00:00Z",
            "platforms": {
                "darwin-aarch64": {"signature": "old-sig", "url": "old-url"},
                "windows-x86_64": {"signature": "win-sig", "url": "win-url"},
            },
        }

    def test_fresh_manifest_without_a_remote(self):
        manifest, status = publisher.start_manifest("0.4.1", None, "now", None, {"darwin-aarch64"})
        self.assertEqual(manifest["version"], "0.4.1")
        self.assertEqual(manifest["notes"], "Release 0.4.1")
        self.assertEqual(manifest["platforms"], {})
        self.assertIsNone(status)

    def test_same_version_keeps_other_platforms(self):
        manifest, status = publisher.start_manifest(
            "0.4.0", None, "now", self.remote(), {"darwin-aarch64"}
        )
        # The platform being republished is carried over too, then overwritten by the
        # caller; the point is that windows-x86_64 survives a mac-only publish.
        self.assertIn("windows-x86_64", manifest["platforms"])
        self.assertIn("keeping: windows-x86_64", status)

    def test_same_version_preserves_remote_notes_and_pub_date(self):
        manifest, _ = publisher.start_manifest("0.4.0", None, "now", self.remote(), set())
        self.assertEqual(manifest["notes"], "remote notes")
        self.assertEqual(manifest["pub_date"], "2026-08-05T10:00:00Z")

    def test_explicit_notes_override_the_remote(self):
        manifest, _ = publisher.start_manifest("0.4.0", "mine", "now", self.remote(), set())
        self.assertEqual(manifest["notes"], "mine")

    def test_new_version_discards_remote_platforms(self):
        manifest, status = publisher.start_manifest(
            "0.4.1", None, "now", self.remote(), {"darwin-aarch64"}
        )
        self.assertEqual(manifest["platforms"], {})
        self.assertEqual(manifest["pub_date"], "now")
        self.assertIn("replacing remote manifest", status)


class FindFilesTests(unittest.TestCase):
    def test_skips_hdiutil_leftovers_and_app_internals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "Memento.app" / "Contents" / "MacOS").mkdir(parents=True)
            (root / "Memento.app" / "Contents" / "MacOS" / "nested.dmg").write_bytes(b"x")
            (root / "rw.11629.Memento_0.4.0_aarch64.dmg").write_bytes(b"x")
            keep = root / "Memento_0.4.0_aarch64.dmg"
            keep.write_bytes(b"x")
            found = publisher.find_files([root], publisher.INSTALLER_SUFFIXES)
        self.assertEqual(found, [keep])


class UploadMetadataTests(unittest.TestCase):
    def test_content_types_cover_the_updater_artifacts(self):
        self.assertEqual(publisher.content_type_for("Memento.app.tar.gz"), "application/gzip")
        self.assertEqual(publisher.content_type_for("Memento.app.tar.gz.sig"), "text/plain")
        self.assertEqual(publisher.content_type_for("latest.json"), "application/json")
        self.assertEqual(publisher.content_type_for("Memento.dmg"), "application/x-apple-diskimage")

    def test_public_url_is_path_style(self):
        url = publisher.public_url("https://obs.example.ru/", "bucket", "prefix/latest.json")
        self.assertEqual(url, "https://obs.example.ru/bucket/prefix/latest.json")


if __name__ == "__main__":
    unittest.main()
