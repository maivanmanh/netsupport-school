#!/usr/bin/env python3
"""Package/deploy development Students using the running Teacher's private IPC.

No SSH, password, shell interpolation, or UI interaction. Exit 0 means every
selected replacement reconnected; transfer completion alone is not success.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import socket
import struct
import sys
import time
import uuid
import zipfile


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(65536), b""):
            digest.update(block)
    return digest.hexdigest()


def architecture(executable, platform):
    with open(executable, "rb") as f:
        if platform == "windows":
            if f.read(2) != b"MZ":
                raise ValueError("Expected a Windows PE executable")
            f.seek(0x3C)
            offset = struct.unpack("<I", f.read(4))[0]
            f.seek(offset)
            if f.read(4) != b"PE\0\0":
                raise ValueError("Invalid PE header")
            machine = struct.unpack("<H", f.read(2))[0]
            result = {0x8664: "x86_64", 0xAA64: "aarch64"}.get(machine)
        elif platform == "linux":
            header = f.read(64)
            if (len(header) != 64 or header[:7] != b"\x7fELF\x02\x01\x01"
                    or header[7] not in (0, 3)):
                raise ValueError("Expected a little-endian 64-bit Linux ELF executable")
            kind, machine, version = struct.unpack_from("<HHI", header, 16)
            header_size = struct.unpack_from("<H", header, 52)[0]
            if kind not in (2, 3) or version != 1 or header_size != 64:
                raise ValueError("Expected an ELF executable or PIE")
            result = {62: "x86_64", 183: "aarch64"}.get(machine)
        elif platform == "macos":
            magic, cpu = struct.unpack("<II", f.read(8))
            if magic != 0xFEEDFACF:
                raise ValueError("Expected a thin 64-bit Mach-O executable")
            result = {0x100000C: "aarch64", 0x1000007: "x86_64"}.get(cpu)
        else:
            raise ValueError("Unsupported Student platform")
    if result is None:
        raise ValueError("Unsupported Student architecture")
    return result


def package(args):
    source = Path(args.source).resolve(strict=True)
    if source.is_dir():
        if source.suffix != ".app":
            raise ValueError("Directory source must be Classroom Student.app")
        executable = source / "Contents/MacOS/ClassroomStudent"
        platform = "macos"
        files = sorted(p for p in source.rglob("*") if not p.is_dir())
    else:
        executable = source
        with source.open("rb") as f:
            magic = f.read(4)
        platform = "linux" if magic == b"\x7fELF" else (
            "windows" if source.suffix.lower() == ".exe" else "macos"
        )
        files = [source]
    arch = architecture(executable, platform)
    output = Path(args.output).resolve()
    if output == source or source in output.parents:
        raise ValueError("Output ZIP must be outside the source bundle")
    output.parent.mkdir(parents=True, exist_ok=True)
    relative_exe = str(Path(source.name) / executable.relative_to(source)) if source.is_dir() else source.name
    manifest = {"format": 1, "os": platform, "arch": arch,
                "version": args.version, "root": source.name,
                "executable": relative_exe, "sha256": sha256(executable)}
    temporary = output.with_name(output.name + ".part")
    try:
        with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("manifest.json", json.dumps(manifest))
            for path in files:
                if path.is_symlink():
                    raise ValueError("Symlinks are not supported in Student dev packages")
                relative = Path(source.name) / path.relative_to(source) if source.is_dir() else Path(source.name)
                if any(p in {"logs", "archive", "__pycache__"} for p in relative.parts) or path.name == ".DS_Store":
                    continue
                archive.write(path, relative.as_posix())
        if temporary.stat().st_size > 512 * 1024 * 1024:
            raise ValueError("Package exceeds 512 MiB")
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    print(json.dumps({"package": str(output), **manifest}, ensure_ascii=False))


class IPC:
    def __init__(self, endpoint):
        self.endpoint = Path(endpoint).resolve(strict=True)
        with self.endpoint.open() as f:
            descriptor = json.load(f)
        self.socket = socket.create_connection(("127.0.0.1", descriptor["port"]), timeout=10)
        self.send({"type": "hello", "protocol": 1, "token": descriptor["token"], "ui_role": "automation"})
        self.state = self.read()
        self.counter = 0

    def send(self, value):
        data = json.dumps(value).encode()
        self.socket.sendall(struct.pack("!I", len(data)) + data)

    def exact(self, size):
        result = bytearray()
        while len(result) < size:
            block = self.socket.recv(size - len(result))
            if not block:
                raise EOFError("Teacher disconnected")
            result.extend(block)
        return result

    def read(self):
        size = struct.unpack("!I", self.exact(4))[0]
        if not 0 < size <= 8 * 1024 * 1024:
            raise ValueError("Invalid Teacher IPC frame size")
        value = json.loads(self.exact(size))
        if value.get("type") == "state":
            self.state = value
        return value

    def command(self, action, selected=(), data=None):
        self.counter += 1
        self.send({"id": self.counter, "action": action, "selected": list(selected), "input": data or {}})
        while True:
            reply = self.read()
            if reply.get("type") == "ack" and reply.get("id") == self.counter:
                if not reply.get("ok"):
                    raise RuntimeError(reply.get("error") or self.state.get("error") or "Teacher rejected command")
                return reply


def deploy(args):
    client = IPC(args.endpoint)
    try:
        if args.list:
            caps = client.state.get("tools", {}).get("capabilities", {})
            print(json.dumps([dict(row, capability=caps.get(row["id"], {})) for row in client.state["students"]], ensure_ascii=False, indent=2))
            return
        package_path = Path(args.package).resolve(strict=True)
        with zipfile.ZipFile(package_path) as archive:
            manifest = json.loads(archive.read("manifest.json"))
        if package_path.stat().st_size > 512 * 1024 * 1024:
            raise ValueError("Package exceeds 512 MiB")
        if client.state["phase"] != "idle":
            raise RuntimeError("Stop sharing before dev deployment")
        if client.state.get("observe") and client.state["observe"].get("phase") not in (None, "closed", "ended", "idle", "error"):
            raise RuntimeError("Close Observe windows before dev deployment")
        rows = [s for s in client.state["students"] if s["online"]]
        caps = client.state.get("tools", {}).get("capabilities", {})
        compatible = [s for s in rows if caps.get(s["id"], {}).get("dev_update") == 1
                      and caps[s["id"]].get("os") == manifest["os"] and caps[s["id"]].get("arch") == manifest["arch"]]
        if args.all_compatible:
            selected = [s["id"] for s in compatible]
        else:
            selected = []
            for requested in args.student:
                matches = [s for s in compatible if requested in (s["id"], s["ip"])]
                if len(matches) != 1:
                    raise ValueError(f"Student {requested} must be online, compatible and updater-enabled (use --list)")
                if matches[0]["id"] not in selected:
                    selected.append(matches[0]["id"])
        if not selected:
            raise ValueError("No compatible online Students")
        outbox = client.endpoint.parent / "outbox"
        outbox.mkdir(exist_ok=True)
        staged = outbox / uuid.uuid4().hex
        shutil.copyfile(package_path, staged)
        try:
            reply = client.command("tool_update", selected, {"path": str(staged), "name": package_path.name})
        except BaseException:
            # The agent owns accepted sources; only remove a rejected copy.
            staged.unlink(missing_ok=True)
            raise
        job_id = reply.get("job_id")
        if not job_id:
            raise RuntimeError("Teacher agent lacks dev-update job IDs; upgrade Teacher first")
        deadline = time.monotonic() + args.timeout
        previous = None
        while time.monotonic() < deadline:
            client.read()
            jobs = client.state.get("tools", {}).get("jobs", [])
            job = next((j for j in jobs if j["id"] == job_id), None)
            if not job:
                continue
            targets = job["targets"]
            view = [(t["id"], t["status"], t["done"], t.get("error", ""), t.get("reply", "")) for t in targets]
            if view != previous:
                print(json.dumps({"job_id": job_id, "version": manifest["version"], "targets": targets}, ensure_ascii=False), flush=True)
                previous = view
            if all(t["status"] in {"complete", "error", "cancelled"} for t in targets):
                if not all(t["status"] == "complete" for t in targets):
                    raise RuntimeError("One or more Students failed to update; see the per-Student result above")
                print("All selected Students restarted and confirmed the new build.")
                return
        raise TimeoutError(f"Deployment timeout; job {job_id} continues on Teacher. Do not resend until its result is known.")
    finally:
        client.socket.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    pack = sub.add_parser("package", help="Create a verified Student ZIP from an exe, binary or .app")
    pack.add_argument("source")
    pack.add_argument("--version", required=True, help="Dev build label, e.g. cursor-fix-20261005")
    pack.add_argument("--output", required=True)
    deploy_parser = sub.add_parser("deploy", help="Deploy through running Teacher, without SSH/UI")
    deploy_parser.add_argument("--endpoint", required=True, help="Teacher teacher-endpoint.json (private local IPC)")
    deploy_parser.add_argument("--package")
    selection = deploy_parser.add_mutually_exclusive_group()
    selection.add_argument("--student", action="append", default=[], help="Student ID or IP; repeat for multiple Students")
    selection.add_argument("--all-compatible", action="store_true")
    deploy_parser.add_argument("--list", action="store_true")
    deploy_parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    if args.command == "package":
        package(args)
    else:
        if not args.list and (not args.package or not (args.student or args.all_compatible)):
            parser.error("deploy requires --package and --student or --all-compatible (or --list)")
        deploy(args)


if __name__ == "__main__":
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        sys.exit(1)
