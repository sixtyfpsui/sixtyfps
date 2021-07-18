#!/usr/bin/python3

# LICENSE BEGIN
# Copyright (c) 2021 Tobias Hunger <tobias.hunger@gmail.com>
#
# SPDX-License-Identifier: GPL-3.0-only
# This file is also available under commercial licensing terms.
# Please contact info@sixtyfps.io for more information.
# LICENSE END

import datetime
import subprocess
import typing


def parse_message(input: str) -> typing.Tuple[str, str]:
    subject = ""
    message = ""

    for l in input.rstrip().split('\n'):
        if not l.startswith("    "):
            continue
        if subject == "":
            subject = l[4:]
            continue

        message += f"{l[4:]}\n"

    return (subject, message.strip())


def parse_git(input: str) -> typing.Sequence[typing.Tuple[str, str, str]]:
    result: typing.List[typing.Tuple[str, str, str]] = []

    author = ""
    message = ""

    for l in input.split('\n'):
        if l.startswith("commit "):
            if author != "" and message != "":
                (summary, msg) = parse_message(message)
                result.append((author, summary, msg))
            author = ""
            message = ""
        elif l == "" or l.startswith("Date: "):
            continue
        elif l.startswith("Author: "):
            author = l[8:].strip()
        elif l.startswith("    "):
            message = f"{message}{l}\n"
        else:
            print(f"ERROR: Unknown line \"{l}\".")

    return result


def generate_header(start_date: datetime.datetime, end_date: datetime.datetime) -> str:
    start_str = f"{start_date.month}.{start_date.day}.{start_date.year}"
    end_str = f"{end_date.month}.{end_date.day}.{end_date.year}"
    timestamp_str = f"{end_date.year}-{end_date.month:02}-{end_date.day:02}T{end_date.hour:02}:{end_date.minute:02}:{end_date.second:02}+01:00"
    return f"""<!DOCTYPE html>
<html lang="en">

<head>
    <meta name="description" content="Changelog for {end_str}" />
    <title>{start_str} to {end_str}</title>
</head>
<!--
    <date>{timestamp_str}</date>
    <discussion_id></discussion_id>
    <author></author>
-->

<body>

<div></div>

"""


def generate_progress(git: typing.Sequence[typing.Tuple[str, str, str]]) -> str:
    result = """<h3>Progress</h3>

<p>Progress was made in the following areas:</p>

<ul>
"""
    for (_, summary, message) in git:
        msg_str = ""
        if message != "":
            tmp = message.replace("\n\n", "</p>\n<p>")
            msg_str = f"\n\n    <p>{tmp}</p>"
        result = f"{result}<li>{summary}{msg_str}</li>\n\n"
    result = f"{result}</ul>\n"

    return result


def generate_statistics(git: typing.Sequence[typing.Tuple[str, str, str]]) -> str:
    commit_count = len(git)

    authors: typing.Dict[str, int] = {}
    for (author, _, _) in git:
        if author not in authors:
            authors[author] = 1
        else:
            authors[author] += 1

    return f"""<h3>Statistics</h3>

<p>{commit_count} patches were commited by {len(authors)} authors.</p>

"""


def generate_footer():
    return "</body>"


end_date = datetime.datetime.now().replace(hour=23, minute=59, second=59, microsecond=999999) 
start_date = (end_date - datetime.timedelta(days=7)).replace(hour=0, minute=0, second=0, microsecond=0)

result = subprocess.run(
    [
        "/usr/bin/git",
        "log",
        f"--since={start_date.isoformat()}",
        f"--until={end_date.isoformat()}"
    ],
    capture_output=True, check=True)

git_log = parse_git(result.stdout.decode('utf8'))

result = generate_header(start_date, end_date)
result += generate_progress(git_log)
result += generate_statistics(git_log)
result += generate_footer()

timestamp = end_date + datetime.timedelta(days=1)
with open(f"{timestamp.year}-{timestamp.month:02}-{timestamp.day:02}.html", "w") as f:
    f.write(result)
