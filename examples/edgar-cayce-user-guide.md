# Edgar Cayce Bot — User Guide

A Discord bot that answers questions grounded in ingested documents.

## Commands

| Command | What it does |
|---------|-------------|
| `/edgar ask <topic> <question>` | Ask a question about a topic |
| `/edgar sources` | See what documents are available |
| `/edgar thread [name]` | Start a new conversation thread |
| `/edgar clear` | Reset your current session |

## Asking a Question

```
/edgar ask topic:akash-deployments question:How do I deploy a container on Akash?
```

The `topic` field autocompletes — start typing and pick from the list.

The bot reads through the ingested source documents, finds the relevant sections, and synthesizes an answer grounded in the actual content (not general training data).

## Examples

```
/edgar ask topic:akash-deployments question:What are the hardware requirements for running a provider?
/edgar ask topic:akash-deployments question:How does the bid pricing system work?
/edgar ask topic:akash-deployments question:What is an SDL file?
```

## Tips

- Be specific with your question — the bot searches through real documents, so precise questions get better answers
- Use `/edgar sources` to see which topics are available before asking
- Start a `/edgar thread` if you want a focused multi-turn conversation
- `/edgar clear` resets context if responses seem off-track
