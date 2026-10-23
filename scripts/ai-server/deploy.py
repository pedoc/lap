#!/usr/bin/env python3
"""Prepare and run a pinned GPU server. Uses only the Python standard library."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import secrets
import shlex
import shutil
import subprocess
import sys

CATALOG = json.loads(Path(__file__).with_name("profiles.json").read_text(encoding="utf-8"))


def version_pair(value):
    match = re.search(r"(\d+)\.(\d+)", value)
    if not match:
        raise ValueError("无法读取 CUDA 版本")
    return tuple(map(int, match.groups()))


def check_platform():
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64"):
        raise ValueError("安装与运行要求 Linux x86_64 GPU 服务器；其他系统可用 --dry-run 查看计划")
    if sys.version_info[:2] != (3, 12):
        raise ValueError("请在 Python 3.12 镜像中执行，不修改系统 Python")


def doctor(profile):
    check_platform()
    required = ["git", "nvidia-smi"]
    if profile["engine"] == "sglang":
        required += ["cargo", "rustup", "protoc", "g++", "nvcc", "ffmpeg"]
    missing = [name for name in required if not shutil.which(name)]
    if missing:
        raise ValueError("缺少工具：" + ", ".join(missing) + "。按 README 的前置环境步骤准备后重试")
    result = subprocess.run(["nvidia-smi"], check=True, text=True, capture_output=True)
    match = re.search(r"CUDA Version:\s*([\d.]+)", result.stdout)
    if not match or version_pair(match.group(1)) < version_pair(profile["minimumDriverCuda"]):
        raise ValueError("当前固定后端要求驱动报告的 CUDA 支持至少为 " + profile["minimumDriverCuda"])
    if profile["engine"] == "sglang":
        toolkit = subprocess.run(["nvcc", "--version"], check=True, text=True, capture_output=True)
        compiler = re.search(r"release\s+([\d.]+)", toolkit.stdout)
        if not compiler or version_pair(compiler.group(1)) < (13, 0):
            raise ValueError("Mage 当前固定包还要求 CUDA 13 开发工具，请使用对应 devel 镜像；只升级驱动不足以替换 nvcc")
        subprocess.run(["rustup", "run", "1.90.0", "cargo", "--version"], check=True)
    subprocess.run(["nvidia-smi", "--query-gpu=name,memory.total,driver_version", "--format=csv,noheader"], check=True)
    print("前置工具与驱动预检通过；这不代表显存和真实推理已经验收", flush=True)


def paths(root, name):
    folder = root / name
    return folder, folder / "venv" / "bin" / "python", folder / "server-api-key"


def run(command, env=None, cwd=None):
    print("+ " + shlex.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), env=env, cwd=cwd, check=True)


def install(root, name, profile, dry_run=False):
    folder, python, _ = paths(root, name)
    source = folder / "sglang-source"
    commands = [[sys.executable, "-m", "venv", str(folder / "venv")],
                [str(python), "-m", "pip", "install", "pip==25.3", "wheel==0.45.1", "setuptools==80.9.0"]]
    if profile["engine"] == "sglang":
        commands += [["git", "init", str(source)],
                     ["git", "-C", str(source), "fetch", "--depth", "1", profile["source"], profile["sourceRevision"]],
                     ["git", "-C", str(source), "checkout", "--detach", profile["sourceRevision"]]]
    commands += [[str(python), "-m", "pip", "install", *profile["packages"]]]
    if profile["engine"] == "sglang":
        # Base inference dependencies suffice; diffusion/tracing extras are unrelated to Mage serving.
        commands += [[str(python), "-m", "pip", "install", "-e", str(source / "python")]]
    commands += [[str(python), "-m", "pip", "check"],
                 [str(python), "-c", "import torch; print('torch:',torch.__version__,'CUDA:',torch.version.cuda); assert torch.cuda.is_available(), 'CUDA unavailable'"]]
    if dry_run:
        print(json.dumps({"profile": name, "model": profile["model"], "revision": profile["modelRevision"],
                          "minimumDriverCuda": profile["minimumDriverCuda"], "commands": commands}, ensure_ascii=False, indent=2))
        return
    doctor(profile)
    folder.mkdir(parents=True, exist_ok=True)
    stamp = hashlib.sha256(json.dumps(profile, sort_keys=True).encode()).hexdigest()
    owner = folder / "installation.json"
    if owner.exists():
        previous = json.loads(owner.read_text(encoding="utf-8"))
        if not isinstance(previous, dict) or previous.get("profileHash") != stamp:
            raise ValueError("该目录属于另一版本配置，请选择新的 --root，避免覆盖已有环境")
    if any(folder.iterdir()) and not owner.exists():
        raise ValueError("部署目录已有内容且没有安装标记，请使用空的部署目录")
    # Mark ownership before work so interrupted downloads/installations can be retried.
    owner.write_text(json.dumps({"profileHash": stamp, "status": "installing"}), encoding="utf-8")
    env = os.environ.copy()
    for variable in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV"):
        env.pop(variable, None)
    env["PIP_CACHE_DIR"] = str(root / "pip-cache")
    env["TMPDIR"] = str(root / "tmp")
    Path(env["TMPDIR"]).mkdir(parents=True, exist_ok=True)
    if profile["engine"] == "sglang":
        env["RUSTUP_TOOLCHAIN"] = "1.90.0"
        env["CUDA_HOME"] = str(Path(shutil.which("nvcc")).resolve().parent.parent)
    for command in commands:
        run(command, env=env)
    frozen = subprocess.run([str(python), "-m", "pip", "freeze"], text=True, capture_output=True, check=True)
    (folder / "resolved-requirements.txt").write_text(frozen.stdout, encoding="utf-8")
    owner.write_text(json.dumps({"profileHash": stamp, "status": "installed", "profile": profile}, indent=2), encoding="utf-8")
    print("安装和 CUDA 导入检查完成；下一步 serve 启动并 probe 验收真实推理")


def serve_command(python, profile, model, host, port, context, memory):
    if profile["engine"] == "sglang":
        # Inject the key inside Python rather than exposing it in process command-line arguments.
        wrapper = "import os,runpy,sys;sys.argv=['sglang.launch_server',*sys.argv[1:],'--api-key',os.environ.pop('LAP_AI_API_KEY')];runpy.run_module('sglang.launch_server',run_name='__main__')"
        return [str(python), "-c", wrapper, "--model-path", model, "--served-model-name", profile["servedModel"],
                "--host", host, "--port", str(port), "--context-length", str(context),
                "--max-running-requests", "1", "--mem-fraction-static", str(memory), "--trust-remote-code"]
    command = [str(python), "-m", "vllm.entrypoints.openai.api_server", "--model", model,
            "--served-model-name", profile["servedModel"], "--host", host, "--port", str(port),
            "--max-model-len", str(context), "--max-num-seqs", "1", "--gpu-memory-utilization", str(memory), "--enforce-eager"]
    if profile.get("task") == "embedding":
        return command + ["--runner", "pooling", "--convert", "embed",
                          "--override-pooler-config", json.dumps(profile["pooling"])]
    return command + ["--limit-mm-per-prompt", json.dumps({"image": 8, "video": 0})]


def load_or_create_key(path):
    try:
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError:
        key = path.read_text(encoding="utf-8").strip()
        if len(key) < 24:
            raise ValueError("已有 API Key 文件无效，请检查该文件；不会自动覆盖")
        return key
    with os.fdopen(fd, "w", encoding="utf-8") as file:
        key = "lap-" + secrets.token_urlsafe(32)
        file.write(key + "\n")
    return key


def serve(args, profile):
    folder, python, key_file = paths(args.root, args.profile)
    port = args.port or profile["port"]
    model = str(args.model_path.resolve()) if args.model_path else profile["model"]
    memory = args.memory if args.memory is not None else profile.get("defaultMemory", 0.8)
    command = serve_command(python, profile, model, args.host, port, args.context, memory)
    if args.dry_run:
        print(json.dumps({"command": command, "keyFile": str(key_file), "modelRevision": profile["modelRevision"],
                          "protocol": "OpenAI Embeddings" if profile.get("task") == "embedding" else "OpenAI Chat Completions",
                          "servedModel": profile["servedModel"], "baseUrl": f"http://127.0.0.1:{port}/v1"}, ensure_ascii=False, indent=2))
        return
    doctor(profile)
    if not python.is_file():
        raise ValueError("运行环境未安装，请先执行 install")
    owner = folder / "installation.json"
    stamp = hashlib.sha256(json.dumps(profile, sort_keys=True).encode()).hexdigest()
    record = json.loads(owner.read_text(encoding="utf-8")) if owner.is_file() else {}
    if record.get("status") != "installed" or record.get("profileHash") != stamp:
        raise ValueError("该环境尚未完成当前固定版本的安装，请先执行 install")
    if args.model_path and not (args.model_path / "config.json").is_file():
        raise ValueError("--model-path 必须是包含 config.json 和完整处理器／权重的模型目录")
    env = os.environ.copy()
    for variable in ("PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV"):
        env.pop(variable, None)
    env["HF_HOME"] = str(args.root / "hf-cache")
    if profile["engine"] == "sglang":
        env["RUSTUP_TOOLCHAIN"] = "1.90.0"
        env["CUDA_HOME"] = str(Path(shutil.which("nvcc")).resolve().parent.parent)
    if not args.model_path:
        print("准备固定版本模型；首次下载可能较大，已有缓存会复用……", flush=True)
        # Snapshot before loading so remote custom model code is fixed to an exact revision.
        script = "import sys;from huggingface_hub import snapshot_download;print(snapshot_download(sys.argv[1],revision=sys.argv[2]))"
        result = subprocess.run([str(python), "-c", script, profile["model"], profile["modelRevision"]], env=env, text=True, capture_output=True)
        if result.returncode:
            raise ValueError("模型下载失败。检查服务器访问 Hugging Face 的网络或使用 --model-path 指向已下载目录；不会输出可能含凭据的下载日志")
        model = result.stdout.strip().splitlines()[-1]
        command = serve_command(python, profile, model, args.host, port, args.context, memory)
    key = load_or_create_key(key_file)
    env["LAP_AI_API_KEY" if profile["engine"] == "sglang" else "VLLM_API_KEY"] = key
    print(f"模型：{profile['servedModel']}；API Key 文件：{key_file}（不在日志显示内容）", flush=True)
    print(f"目标 API：http://127.0.0.1:{port}/v1；启动后按 README 验证该模型能力及完整业务链路", flush=True)
    # Remain in the foreground; Ctrl+C and terminal process ownership stay explicit.
    run(command, env=env)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("doctor", "install", "serve"))
    parser.add_argument("profile", choices=tuple(CATALOG["profiles"]))
    parser.add_argument("--root", type=Path, default=Path.home() / "lap-ai")
    parser.add_argument("--dry-run", action="store_true", help="只显示安装或启动计划，不安装、不下载、不生成密钥")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int)
    parser.add_argument("--context", type=int, default=8192)
    parser.add_argument("--memory", type=float, help="显存使用比例；生成模型默认 0.8，文本向量模型默认 0.15")
    parser.add_argument("--model-path", type=Path)
    args = parser.parse_args()
    if args.context < 1024 or args.memory is not None and not 0 < args.memory < 1 or args.port is not None and not 1 <= args.port <= 65535:
        parser.error("context 至少 1024；memory 在 0 与 1 之间；port 在 1–65535 之间")
    args.root = args.root.expanduser().resolve()
    profile = CATALOG["profiles"][args.profile]
    try:
        if args.action == "doctor":
            doctor(profile)
        elif args.action == "install":
            install(args.root, args.profile, profile, args.dry_run)
        else:
            serve(args, profile)
    except (ValueError, OSError, subprocess.CalledProcessError) as exc:
        print("部署未完成：" + str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
