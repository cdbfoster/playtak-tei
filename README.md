# PlayTak.com TEI Client

This tool brings [PlayTak.com](https://playtak.com) playability to any engine that supports the TEI protocol.

## Commands

The tool accepts three commands:
* `list` - Lists the available seeks and exits.
* `accept` - Accepts a currently open seek.
* `seek` - Posts a new seek.

All commands will login as `Guest` by default, and since the server will recognize repeat connections for some time, it should be possible to receive the same guest login number across multiple runs of the tool, provided the runs are within some amount of time of each other (a few hours).

All commands may be provided login credentials for a named account too, in the form:

```bash
$ playtak-tei list -u myusername -p mypassword.
```

Consult each command's `--help` for options.

### Engine Arguments

The `accept` and `seek` commands both require engine arguments to invoke.  The tool will invoke all trailing arguments as passed.  For example:

```bash
$ playtak-tei accept -s 123456 path/to/my/engine arg1 arg2 arg3
```

will execute the binary `path/to/my/engine` with the arguments `arg1 arg2 arg3`.

## Notes

* When accepting a seek, or a when seek that we've posted is accepted, the game will start immediately.  If the engine player is the next to move, the tool will query the engine for a move right away; no interaction with the tool after startup is required.
* When a game ends, the tool will print the result and exit.
* If the tool loses connection in the middle of a game, it will attempt to resume the game upon the next run.  Usually this only requires running the tool again, with the arguments unchanged.
* Debug logging can be turned on using an environment variable: `RUST_LOG=playtak_tei=debug`. Among other things, this will display the communication between the tool, PlayTak.com, and the engine.