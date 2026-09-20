# Which model

`polygo models` lists the options with sizes and notes. The workflow is the same whichever you pick.

| | Size | Good for |
|---|---|---|
| `qwen3:8b` (default) | 5 GB | UI strings into major languages. In a live test: 8 of 8 Polish plural forms, 6 of 8 Russian, rough Bulgarian ("Достъп за достъп"). |
| `gemma4` | 9.6 GB | Smaller languages, Slavic plurals, anything customer-facing. |
| `openai/…`, `anthropic/…` | API | Same as above, no local GPU needed. |

`check` catches structural mistakes, not a wrong word, so step up from the default for anything a customer will read.

```sh
polygo use gemma4                               # pulls through Ollama with a progress bar, writes polygo.toml
polygo use qwen3:14b --global                   # also the default for future `polygo init`
polygo use openai/gpt-4o-mini --api-key sk-...  # key stored in ~/.config/polygo/credentials.toml (0600)
polygo use anthropic/claude-sonnet-5            # or export ANTHROPIC_API_KEY
polygo use openai/llama-3.3-70b --base-url https://api.groq.com/openai/v1   # any OpenAI-compatible server
```
