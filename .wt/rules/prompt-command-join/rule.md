# Newline-joined PROMPT_COMMAND

Profiles commonly use `PROMPT_COMMAND="history -a; $PROMPT_COMMAND"`, leaving a
trailing `; `. Appending `; __kea_prompt` then produces `; ;`, a Bash syntax
error at every prompt: the user's own hook stops running and Kea never receives
a ready report. Join the existing scalar value with newlines, which are valid
after `;`, `&` or another newline. The array form is already safe. The shell
unit test feeds such values to a real Bash.
