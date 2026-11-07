"""Validate explicitly reviewed additive migrations before assistant-disabled rollback."""
import hashlib
import json
import re
import subprocess
import sys


def check(root, live, previous, before, current, saver, evidence):
    def git(*args):
        return subprocess.check_output(["git", "-C", root, *args], stderr=subprocess.DEVNULL)

    marker_path = "deploy/assistant-compatibility.json"
    marker = json.loads(git("show", f"{live}:{marker_path}"))
    fingerprint_keys = ("graphVersion", "workflowVersion", "saverVersion", "checkpointSchema", "checkpointSchemaVersion", "invoiceDeliveryVersion", "batchVersion")
    expected = ("office-assistant-v1", 2, "1.1.2", "assistant_checkpoint", 5, 1, 1)
    if tuple(marker.get(key) for key in fingerprint_keys) != expected or saver != "5":
        raise ValueError("unsupported graph/checkpointer fingerprint")
    try:
        prior = json.loads(git("show", f"{previous}:{marker_path}"))
    except subprocess.CalledProcessError:
        raise ValueError("previous release has no supported assistant fingerprint")
    if any(prior.get(key) != marker[key] for key in fingerprint_keys):
        raise ValueError("previous graph/checkpointer/delivery/batch fingerprint differs")
    # Disabling admission must leave existing invoice work executable after rollback.
    for revision in (live, previous):
        git("cat-file", "-e", f"{revision}:apps/api/src/modules/invoices/delivery.rs")
        jobs = git("show", f"{revision}:apps/api/src/jobs/kinds.rs").decode()
        for dispatch in ("INVOICE_DELIVERY => return crate::modules::invoices::delivery::run", "INVOICE_DELIVERY_SWEEP => return crate::modules::invoices::delivery::sweep"):
            if dispatch not in jobs:
                raise ValueError("release lacks invoice delivery worker dispatch")
        for name in ("batches", "batch_repo", "batch_handlers", "batch_dto"):
            git("cat-file", "-e", f"{revision}:apps/api/src/modules/assistant/{name}.rs")
        guard = git("show", f"{revision}:apps/api/src/http/assistant.rs").decode()
        if "Assistant-Batch-Token" not in guard or "batch_repo::claim(" not in guard:
            raise ValueError("release lacks batch controller admission")
        repo = git("show", f"{revision}:apps/api/src/modules/assistant/repo.rs").decode()
        if "batch_repo::expire(" not in repo or "batch_repo::prune(" not in repo:
            raise ValueError("release lacks batch cleanup")
    before, current = int(before), int(current)
    if before > current:
        raise ValueError("previous schema is ahead of current schema")
    allowed = {}
    for path, checksum in marker["migrations"].items():
        match = re.fullmatch(r"apps/api/migrations/(\d{14})_(?:assistant_[a-z_]+|invoice_delivery)\.up\.sql", path)
        if not match or not re.fullmatch(r"[0-9a-f]{96}", checksum):
            raise ValueError("invalid reviewed migration entry")
        if hashlib.sha384(git("show", f"{live}:{path}")).hexdigest() != checksum:
            raise ValueError("reviewed migration source checksum changed")
        allowed[int(match[1])] = checksum
    applied = {}
    for line in evidence.strip().splitlines():
        version, checksum = line.split("|")
        version = int(version)
        if version in applied or not before < version <= current or allowed.get(version) != checksum:
            raise ValueError("database includes an unreviewed or changed migration")
        applied[version] = checksum
    required = {version for version in allowed if before < version <= current}
    if set(applied) != required or (before != current and (not applied or max(applied) != current)):
        raise ValueError("incomplete applied migration evidence")
    return "/".join(str(marker[key]) for key in fingerprint_keys)


if __name__ == "__main__":
    try:
        print(check(*sys.argv[1:], sys.stdin.read()))
    except (ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        sys.exit(f"assistant-compatible rollback refused: {error}")
