from __future__ import annotations

import io
from datetime import UTC, date, datetime, time

from tomledit import Document, dump, dumps, load, loads

SECTION_COUNT = 180


def make_fixture() -> str:
    lines = [
        "# Representative input for training release builds.",
        'title = "PGO café"',
        "enabled = true",
        "published = 2026-09-05T18:00:00Z",
        "release_date = 2026-09-05",
        "release_time = 18:00:00",
        "",
    ]
    for section in range(SECTION_COUNT):
        lines.extend(
            [
                f"[section_{section}]",
                f'name = "Section {section}" # display name',
                "# stable ordinal",
                f"ordinal = {section}",
                f"ratio = {section}.125",
                f"enabled = {'true' if section % 2 == 0 else 'false'}",
                f"values = [{section}, {section + 1}, {section + 2}, {section + 3}]",
                (
                    'metadata = { owner = "team", priority = '
                    f'{section % 5}, labels = ["one", "two"] }}'
                ),
                "",
                f"[[section_{section}.members]]",
                f'name = "primary-{section}"',
                f"score = {section * 2}",
                "",
                f"[[section_{section}.members]]",
                f'name = "secondary-{section}"',
                f"score = {section * 2 + 1}",
                "",
            ]
        )
    return "\n".join(lines)


FIXTURE = make_fixture()
NATIVE_DATA: dict[str, object] = {
    f"package_{package}": {
        "name": f"Package {package}",
        "versions": list(range(package, package + 12)),
        "metadata": {
            "active": package % 2 == 0,
            "weight": package + 0.5,
        },
    }
    for package in range(120)
}
NATIVE_DATA["dates"] = {
    "date": date(2026, 9, 5),
    "datetime": datetime(2026, 9, 5, 18, tzinfo=UTC),
    "time": time(18, 30, 15),
}


def train_parsing() -> int:
    checksum = 0
    for _ in range(600):
        checksum += len(Document.parse(FIXTURE))
    for _ in range(400):
        checksum += len(Document.parse(FIXTURE).as_toml())
    return checksum


def train_reads() -> int:
    doc = Document.parse(FIXTURE)
    checksum = 0
    for repetition in range(16_000):
        section_number = repetition % SECTION_COUNT
        section = doc[f"section_{section_number}"]
        read_kind = repetition % 4
        if read_kind == 0:
            checksum += int(section["ordinal"])
            checksum += int(section["values"][repetition % 4])
            checksum += len(section["values"][1:3])
        elif read_kind == 1:
            checksum += len(section["name"].value)
            checksum += len(section["name"].inline_comment or "")
            checksum += len(section["ordinal"].comment or "")
            checksum += int("name" in section)
        elif read_kind == 2:
            metadata = section["metadata"]
            checksum += len(metadata["owner"].value)
            checksum += int(metadata["priority"])
            checksum += len(metadata["labels"][repetition % 2].value)
            checksum += len(metadata.keys())
        else:
            checksum += int(section["members"][repetition % 2]["score"])
            checksum += int(bool(section["enabled"]))
            checksum += int(float(section["ratio"]) * 1_000)
            checksum += len(section.items())
    for _ in range(1_600):
        checksum += len(doc.value)
    return checksum


def train_mutations() -> int:
    checksum = 0
    for repetition in range(240):
        doc = Document.parse(FIXTURE)
        for section_number in range(0, SECTION_COUNT, 9):
            section = doc[f"section_{section_number}"]
            mutation_kind = (section_number // 9) % 4
            if mutation_kind == 0:
                section["ordinal"] = repetition
                section["values"][1:3] = [repetition, repetition + 1]
                section["values"].append(repetition + 2)
            elif mutation_kind == 1:
                section["name"] = f"Updated {repetition}"
                section["name"].inline_comment = "# trained"
                section["metadata"]["owner"] = "release"
                section["metadata"]["labels"].append(f"iteration-{repetition}")
            elif mutation_kind == 2:
                section["ratio"] = repetition + 0.25
                section["enabled"] = repetition % 2 == 0
                section["metadata"]["priority"] = repetition % 5
            else:
                section["members"].insert(
                    1,
                    {"name": "inserted", "score": repetition},
                )
                del section["members"][-1]
        doc["generated"] = {
            "iteration": repetition,
            "enabled": repetition % 2 == 0,
            "label": f"iteration-{repetition}",
            "ratio": repetition + 0.5,
            "values": [1, 2, 3],
        }
        doc["published"] = datetime(2026, 9, 5, 18, repetition % 60, tzinfo=UTC)
        doc["release_date"] = date(2026, 9, repetition % 28 + 1)
        doc["release_time"] = time(18, repetition % 60, repetition % 60)
        checksum += len(doc.as_toml())
    return checksum


def train_io() -> int:
    checksum = 0
    for _ in range(300):
        text = dumps(NATIVE_DATA)
        checksum += len(loads(text))

        buffer = io.BytesIO()
        dump(NATIVE_DATA, buffer)
        buffer.seek(0)
        checksum += len(load(buffer))
    return checksum


def main() -> None:
    checksum = train_parsing()
    checksum += train_reads()
    checksum += train_mutations()
    checksum += train_io()
    if checksum <= 0:
        msg = "PGO workload did not exercise tomledit"
        raise RuntimeError(msg)
    print(f"PGO training complete: {checksum}")


if __name__ == "__main__":
    main()
