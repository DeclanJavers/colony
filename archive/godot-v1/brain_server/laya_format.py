"""How a colonist's decision context becomes Laya questions.

Shared by the live server (server.py) and the fine-tuning data builder (tools/finetune/make_data.py)
so a fine-tuned checkpoint is trained on exactly the questions it is asked in the game.
Change this and you should re-train: bump QUESTIONS_VERSION so old datasets can be told apart.
"""

from __future__ import annotations

QUESTIONS_VERSION = "q1"


def _choice(instructions: str, opts: list[dict]) -> dict:
    return {"type": "choice", "instructions": instructions,
            "criteria": {o["label"]: o.get("description", "") for o in opts}}


def build_questions(ctx: dict) -> dict:
    """ctx = Sim.build_context() output (or a decision-log row: same option fields)."""
    name = ctx.get("colonist_name") or ctx.get("name") or "the colonist"
    questions = {"action": _choice(f"What should {name} do next?", ctx["options"])}
    if ctx.get("build_options"):
        questions["build_what"] = _choice(f"If {name} builds, what should it be?", ctx["build_options"])
    if ctx.get("social_options"):
        questions["with_whom"] = _choice(f"If {name} socializes, who with?", ctx["social_options"])
    return questions


OPTION_FIELDS = {"action": "options", "build_what": "build_options", "with_whom": "social_options"}
PROB_FIELDS = {"action": "probs", "build_what": "build_probs", "with_whom": "social_probs"}


def label_to_id(opts: list[dict]) -> dict[str, str]:
    return {o["label"]: str(o["id"]) for o in opts}


def id_to_label(opts: list[dict]) -> dict[str, str]:
    return {str(o["id"]): o["label"] for o in opts}
