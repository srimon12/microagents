import argparse
import os
import shutil

from huggingface_hub import snapshot_download


def copy_jsons_in_questions(local_dir: str, category: str, dataset: str):
    pdf_dir = os.path.join(local_dir, category, dataset)
    os.makedirs("questions/", exist_ok=True)
    files = [
        os.path.join(pdf_dir, f)
        for f in os.listdir(pdf_dir)
        if os.path.isfile(os.path.join(pdf_dir, f))
    ]
    for file in files:
        shutil.copy(file, "questions/")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--category", required=True, choices=["syn-pdfQA", "real-pdfQA"])
    ap.add_argument(
        "--dataset", required=True, help="Dataset folder name (can include spaces)."
    )
    ap.add_argument(
        "--yes",
        required=False,
        action="store_true",
        help="Skip confirmation for actions.",
    )
    args = ap.parse_args()

    repo_id = "pdfqa/pdfQA-Annotations"
    local_root = "."
    local_dir = os.path.join(local_root, f"annotations_{args.category}__{args.dataset}")

    allow = [
        f"{args.category}/{args.dataset}/**",
    ]

    print(f"==> Repo:      {repo_id}")
    print(f"==> Category:  {args.category}")
    print(f"==> Dataset:   {args.dataset}")
    print(f"==> Local dir: {local_dir}")
    print("==> Downloading subset...")

    snapshot_download(
        repo_id=repo_id,
        repo_type="dataset",
        local_dir=local_dir,
        allow_patterns=allow,
    )

    print(f"\n✅ Downloaded: {local_dir}/")

    copy_jsons_in_questions(local_dir, args.category, args.dataset)

    print("\n✅ Copied questions to: questions/")

    if not args.yes:
        proceed = input(f"About to remove {local_dir}, ok to proceed? [y/n]: ")
        if proceed.lower().strip() == "y":
            shutil.rmtree(local_dir)
    else:
        shutil.rmtree(local_dir)
