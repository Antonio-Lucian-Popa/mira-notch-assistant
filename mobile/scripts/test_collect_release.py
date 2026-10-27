"""A mislabeled or wrong-version APK must not become a public release asset."""
import json
from pathlib import Path
import tempfile
import unittest
from collect_release import collect


class CollectReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.apk = self.root / "apk"
        self.apk.mkdir()
        (self.apk / "app-preview-release.apk").write_bytes(b"test apk fixture")
        self.metadata = {"applicationId": "ro.novabytecode.mira.preview", "variantName": "previewRelease",
                         "elements": [{"versionName": "0.7.1-preview", "versionCode": 1042,
                                       "outputFile": "app-preview-release.apk"}]}
        self.write_metadata()

    def write_metadata(self):
        (self.apk / "output-metadata.json").write_text(json.dumps(self.metadata))

    def test_collects_matching_version_and_keeps_bytes(self):
        result = collect("0.7.1", 1042, self.apk, self.root / "release")
        self.assertEqual(result.name, "mira_0.7.1_android_preview.apk")
        self.assertEqual(result.read_bytes(), b"test apk fixture")

    def test_rejects_a_renamed_but_stale_apk(self):
        with self.assertRaises(ValueError):
            collect("0.7.2", 1042, self.apk, self.root / "release")
        with self.assertRaises(ValueError):
            collect("0.7.1", 1043, self.apk, self.root / "release")

    def test_rejects_wrong_package_and_unsafe_paths(self):
        self.metadata["applicationId"] = "wrong.package"
        self.write_metadata()
        with self.assertRaises(ValueError):
            collect("0.7.1", 1042, self.apk, self.root / "release")
        self.metadata["applicationId"] = "ro.novabytecode.mira.preview"
        self.metadata["elements"][0]["outputFile"] = "../other.apk"
        self.write_metadata()
        with self.assertRaises(ValueError):
            collect("0.7.1", 1042, self.apk, self.root / "release")


if __name__ == "__main__":
    unittest.main()
