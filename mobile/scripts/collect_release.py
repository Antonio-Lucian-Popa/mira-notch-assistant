"""Verify the actual APK metadata before attaching it to a versioned release."""
import json
import os
from pathlib import Path
import re
import shutil


def collect(version: str, code: int, apk_dir: Path, destination: Path) -> Path:
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) or not 6 <= code <= 2_100_000_000:
        raise ValueError("Expected a release version X.Y.Z and an increasing Android code")
    metadata = json.loads((apk_dir / "output-metadata.json").read_text())
    if metadata.get("applicationId") != "ro.novabytecode.mira.preview" or metadata.get("variantName") != "previewRelease":
        raise ValueError("Unexpected Android package or build variant")
    elements = metadata["elements"]
    if len(elements) != 1:
        raise ValueError("Expected one universal preview APK")
    entry = elements[0]
    if entry["versionName"] != f"{version}-preview" or entry["versionCode"] != code:
        raise ValueError("APK version does not match the release")
    source = (apk_dir / entry["outputFile"]).resolve()
    if source.parent != apk_dir.resolve() or source.suffix != ".apk" or not source.is_file():
        raise ValueError("Missing or unexpected APK output path")
    destination.mkdir(parents=True, exist_ok=True)
    target = destination / f"mira_{version}_android_preview.apk"
    shutil.copyfile(source, target)
    return target


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    print(collect(os.environ["MIRA_ANDROID_VERSION"], int(os.environ["MIRA_ANDROID_VERSION_CODE"]),
                  root / "android/app/build/outputs/apk/preview/release",
                  root / "android/app/build/outputs/mira-release"))
