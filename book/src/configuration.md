# Configuration

To override global configuration parameters, create a `config.toml` file located in your config directory:

- Linux and Mac: `~/.config/helix/config.toml`
- Windows: `%AppData%\helix\config.toml`

> 💡 You can easily open the config file by typing `:config-open` within Helix normal mode.

Example config:

```toml
theme = "onedark"

[editor]
line-number = "relative"
mouse = false

[editor.cursor-shape]
insert = "bar"
normal = "block"
select = "underline"

[editor.file-picker]
hidden = false
```

You can use a custom configuration file by specifying it with the `-c` or
`--config` command line argument, for example `hx -c path/to/custom-config.toml`.
You can reload the config file by issuing the `:config-reload` command. Alternatively, on Unix operating systems, you can reload it by sending the USR1
signal to the Helix process, such as by using the command `pkill -USR1 hx`.

Finally, you can have a `config.toml` local to a project by putting it under a `.helix` directory in your repository.
Its settings will be merged with the configuration directory `config.toml` and the built-in configuration.

### `editor.ime-context-switching` (boolean)

Enables or disables the automatic Input Method Editor (IME) switching feature globally. When set to `true`, Helix will attempt to automatically manage your IME state based on the syntax context.

**Default:** `false`

For detailed configuration of IME switching behavior per language, including smart defaults for common file types, please refer to the [Languages documentation](./languages.md#automatic-input-method-editor-ime-switching).

