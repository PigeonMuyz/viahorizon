#!/usr/bin/env python3
"""Test the actual injected Horizon transport, without Minecraft assets or Java."""
from pathlib import Path
import importlib.util
import json
import shutil
import subprocess

root = Path(__file__).resolve().parents[1]
stage = root / "target" / "horizon-stage"
if not (stage / "Cargo.toml").is_file():
    raise SystemExit("Run tools/native.py check first to create a transformed stage")
spec = importlib.util.spec_from_file_location("vfp_native_tools", root / "tools" / "native.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
stamp = root / "target" / "native-stage.json"
if not stamp.is_file() or json.loads(stamp.read_text())["bundle_sha256"] != native.bundle_digest():
    raise SystemExit("Mod sources differ from the transformed stage; rerun tools/native.py check before testing")
destination = stage / "crates" / "horizon-protocol" / "tests" / "vfp_transport.rs"
destination.parent.mkdir(exist_ok=True)
shutil.copyfile(root / "tests" / "horizon_transport.rs", destination)
shutil.copyfile(root / "tests" / "horizon_features.rs", destination.with_name("vfp_features.rs"))
shutil.copyfile(root / "tests" / "horizon_predictions.rs", destination.with_name("vfp_predictions.rs"))
shutil.copyfile(root / "tests" / "horizon_configuration_chain.rs", destination.with_name("vfp_configuration_chain.rs"))
shutil.copyfile(root / "tests" / "horizon_gameplay_chain.rs", destination.with_name("vfp_gameplay_chain.rs"))
shutil.copyfile(root / "tests" / "horizon_modern.rs", destination.with_name("vfp_modern.rs"))
shutil.copyfile(root / "tests" / "horizon_outbox.rs", destination.with_name("vfp_outbox.rs"))
(destination.parent / "data").mkdir(exist_ok=True)
shutil.copyfile(root / "mixins" / "viahorizon" / "capture_session.rs", destination.parent / "data" / "vfp_capture_session.rs")
subprocess.run(["cargo", "test", "--locked", "-p", "horizon-protocol", "--lib", "--test", "vfp_transport", "--test", "vfp_features", "--test", "vfp_predictions", "--test", "vfp_configuration_chain", "--test", "vfp_gameplay_chain", "--test", "vfp_modern", "--test", "vfp_outbox", "--no-fail-fast", "--", "--test-threads=1"], cwd=stage, check=True)

render_test = stage / "crates" / "horizon-client-render" / "tests" / "vfp_render.rs"
render_test.parent.mkdir(exist_ok=True)
shutil.copyfile(root / "tests" / "horizon_render.rs", render_test)
subprocess.run(["cargo", "test", "--locked", "-p", "horizon-client-render", "--test", "vfp_render", "--", "--test-threads=1"], cwd=stage, check=True)
