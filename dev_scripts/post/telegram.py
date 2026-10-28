#!/usr/bin/env python3
import requests
import sys

def main():
    token = sys.argv[1]

    with open("changelog.md", "r") as fl:
        last_changelog = fl.read().split("\n\n# deew")[0]
        version = last_changelog.split(" ")[2].replace(":\n-", "")
        changelog = "\n".join(last_changelog.split("\n")[1:]).replace("`", '"')

    print('Sending notification...')
    r = requests.post(
        url=f"https://api.telegram.org/bot{token}/sendMessage",
        data={
            "chat_id": "-1003761788743",
            "parse_mode": "html",
            "disable_web_page_preview": "true",
            "text": f'<a href="https://github.com/pcrlnd/deew/releases/tag/{version}">{version}</a> is out!\n\nChangelog:\n<pre><code class="language-json">{changelog}</code></pre>'
        }
    )
    r.raise_for_status()


if __name__ == "__main__":
    main()
