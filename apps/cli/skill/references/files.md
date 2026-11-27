# Files

Matrix room encryption also applies to attachments. Downloads need the session’s room keys when the attachment is encrypted.

## `bm files list`

List recently shared files and the message event ID for each file.

```bash
bm files list '!ops:example.org'
```

## `bm files upload`

Send a local file and return the server event ID. Add `--caption` or post to a `--thread` root. Use `--txn-id` for automated uploads and reuse it when retrying the same upload after exit 6.

```bash
bm files upload '!ops:example.org' ./deploy.log --caption "Deploy log"
```

## `bm files download`

Download the attachment from an event to a new file at `--output`. Returns the path and IDs.

```bash
bm files download '!ops:example.org' '$deploy-event' --output ./deploy.log
```
