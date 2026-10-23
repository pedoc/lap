#!/usr/bin/env python3
"""Probe a configured standard API without installing an SDK or printing keys."""
import argparse
import base64
import json
import math
import os
from pathlib import Path
import urllib.error
import urllib.parse
import urllib.request

ROUTES = {"chat": "chat/completions", "responses": "responses", "messages": "messages", "embeddings": "embeddings"}


def endpoint(base, route):
    parts = urllib.parse.urlsplit(base)
    if parts.scheme not in ("http", "https") or not parts.hostname or parts.username or parts.password or parts.query or parts.fragment:
        raise ValueError("API 基础地址需为不带内嵌凭据、查询或片段的 HTTP(S) 地址")
    base = base.rstrip("/")
    if any(base.endswith("/" + value) for value in ROUTES.values()):
        raise ValueError("请填写基础地址，例如 http://127.0.0.1:8000/v1，不要重复填写接口路径")
    if not parts.path.rstrip("/").endswith("/v1"):
        base += "/v1"
    return base + "/" + route


def image_data(path):
    if path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError("单张测试图片限 8 MiB，请先缩小测试样例")
    data = path.read_bytes()
    if data.startswith(b"\xff\xd8\xff"):
        mime = "image/jpeg"
    elif data.startswith(b"\x89PNG\r\n\x1a\n"):
        mime = "image/png"
    elif data.startswith(b"RIFF") and data[8:12] == b"WEBP":
        mime = "image/webp"
    else:
        raise ValueError("图片测试支持 JPEG、PNG、WebP，请先转换其他格式")
    return mime, base64.b64encode(data).decode("ascii")


def build_payload(protocol, model, question, images, max_tokens):
    if len(images) > 8:
        raise ValueError("通用采样帧测试最多 8 张，避免无界上传和上下文溢出")
    if protocol == "embeddings":
        if images:
            raise ValueError("OpenAI Embeddings 标准输入是文本／tokens；多模态向量需单独验证其扩展协议")
        return {"model": model, "input": question}
    content = []
    for index, path in enumerate(images):
        mime, data = image_data(path)
        label = f"输入画面 {index + 1}（顺序与调用者给出的时间说明一致）"
        if protocol == "chat":
            content += [{"type": "text", "text": label}, {"type": "image_url", "image_url": {"url": f"data:{mime};base64,{data}"}}]
        elif protocol == "responses":
            content += [{"type": "input_text", "text": label}, {"type": "input_image", "image_url": f"data:{mime};base64,{data}"}]
        else:
            content += [{"type": "text", "text": label}, {"type": "image", "source": {"type": "base64", "media_type": mime, "data": data}}]
    content.append({"type": "input_text" if protocol == "responses" else "text", "text": question})
    if protocol == "responses":
        return {"model": model, "input": [{"role": "user", "content": content}], "max_output_tokens": max_tokens, "store": False}
    return {"model": model, "messages": [{"role": "user", "content": content}], "max_tokens": max_tokens, "stream": False}


def headers(protocol, key, anthropic_version="2026-10-21"):
    result = {"Content-Type": "application/json"}
    if protocol == "messages":
        result.update({"x-api-key": key, "anthropic-version": anthropic_version})
    else:
        result["Authorization"] = "Bearer " + key
    return result


def request_json(url, protocol, key, body=None, timeout=180, anthropic_version="2026-10-21"):
    data = json.dumps(body, ensure_ascii=False).encode("utf-8") if body is not None else None
    request = urllib.request.Request(url, data=data, headers=headers(protocol, key, anthropic_version), method="POST" if body is not None else "GET")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read(8 * 1024 * 1024 + 1)
            if len(raw) > 8 * 1024 * 1024:
                raise ValueError("服务响应超过测试上限")
            return json.loads(raw)
    except urllib.error.HTTPError as exc:
        # Never echo provider bodies: they can contain prompts, signed URLs, or credentials.
        code = exc.code
        exc.close()
        raise ValueError(f"HTTP {code}：服务拒绝请求；检查认证、模型 ID、协议与服务端日志") from None
    except urllib.error.URLError:
        raise ValueError("连接失败：检查服务是否就绪、SSH 隧道、地址和 TLS 证书") from None


def extract(protocol, response):
    if protocol == "embeddings":
        vector = response["data"][0]["embedding"]
        if not vector or not all(isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x) for x in vector):
            raise ValueError("向量包含空值或非有限数值")
        norm = math.sqrt(sum(x * x for x in vector))
        if not math.isfinite(norm) or norm == 0:
            raise ValueError("向量范数无效")
        return f"向量响应通过基本检查，维度={len(vector)}；未验证跨模态对齐与检索质量"
    if protocol == "chat":
        content = response["choices"][0]["message"]["content"]
        if isinstance(content, str):
            return content
        return "\n".join(item["text"] for item in content or [] if item.get("type") == "text")
    if protocol == "messages":
        return "\n".join(item["text"] for item in response["content"] if item.get("type") == "text")
    return "\n".join(block["text"] for item in response["output"] if item.get("type") == "message"
                     for block in item.get("content", []) if block.get("type") == "output_text")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--model", required=True)
    parser.add_argument("--protocol", choices=tuple(ROUTES), default="chat")
    parser.add_argument("--anthropic-version", default="2026-10-21", help="兼容服务需要旧版时可明确指定 2023-06-01")
    secret = parser.add_mutually_exclusive_group(required=True)
    secret.add_argument("--key-file", type=Path)
    secret.add_argument("--key-env", help="读取指定环境变量，命令行不包含密钥值")
    parser.add_argument("--image", type=Path, action="append", default=[])
    parser.add_argument("--question", default="请简要描述画面；没有画面时只回复连接测试成功。")
    parser.add_argument("--max-tokens", type=int, default=256)
    parser.add_argument("--list-only", action="store_true")
    args = parser.parse_args()
    try:
        key = args.key_file.read_text(encoding="utf-8").strip() if args.key_file else os.environ.get(args.key_env, "").strip()
        if not key or not 1 <= args.max_tokens <= 8192:
            raise ValueError("密钥不能为空；max-tokens 范围为 1–8192")
        if args.list_only:
            response = request_json(endpoint(args.base_url, "models"), args.protocol, key, anthropic_version=args.anthropic_version)
            print(json.dumps([row["id"] for row in response["data"]], ensure_ascii=False, indent=2))
        else:
            payload = build_payload(args.protocol, args.model, args.question, args.image, args.max_tokens)
            response = request_json(endpoint(args.base_url, ROUTES[args.protocol]), args.protocol, key, payload, anthropic_version=args.anthropic_version)
            answer = extract(args.protocol, response)
            if not answer.strip():
                raise ValueError("服务未返回可显示文本，可能仅返回推理信息或不兼容的响应")
            print(answer)
    except (ValueError, OSError, KeyError, IndexError, TypeError) as exc:
        print("测试未通过：" + (str(exc) if isinstance(exc, ValueError) else "无法读取输入或响应不符合所选协议"))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
