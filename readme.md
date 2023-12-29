# Rource

Rource is a tool to generate a Gource log file from a git repository or a directory containing git repositories.

Give it a path to a git repository or a directory containing git repositories and it will generate a Gource format log
file.

You can control whether it should recursively search for repositories or not, and you can also specify which
repositories to include or exclude.

Rource also supports username aliases, so you can map various git usernames to cleaner, presentable names to display in
Gource.
It also supports shell expansion, so you can use `~` to refer to your home directory if you prefer.

### Examples

Scan a folder for git repositories recursive, apply two aliases and output to a file:

    rource -p ~/source/my-github-org -r --alias "AdamLeyshon::WelshProgrammer" --alias "SomeOtherUsername::WelshProgrammer" -o output.txt

Scan a folder for git repositories recursive, apply two aliases and pipe directly to Gource:

    rource -p ~/source/my-github-org -r | gource - --log-format custom

Generate log for a single repository and pipe directly to Gource:

    rource -p ~/source/my-github-org/my-repo | gource - --log-format custom

Generate log for this repository and output to a file:

    rource -p ./ -o output.txt

## Building/Installing from source

    git clone https://github.com/AdamLeyshon/rource.git 
    cd rource

    # You can either install it
    cargo install --path .

    # Or build it, the binary will be in ./target/release/rource
    cargo build --release

## Command line options

    Usage: rource [OPTIONS] --path <PATH>
    
    Options:
    -p, --path <PATH>        The path to the git repository/repositories
    -r, --recursive          Recursively search for repositories, by default all repositories in <PATH> will be included
    -i, --include <INCLUDE>  Used with recursive, only process these repositories, cannot be used with --exclude
    -e, --exclude <EXCLUDE>  Used with recursive, exclude these repositories from processing, cannot be used with --include
    -o, --output <OUTPUT>    Output file, defaults to stdout
    --alias <ALIAS>      Add an alias for a user, format is <GIT_USERNAME>::<GOURCE_USERNAME>, you can specify this option multiple times
    -h, --help               Print help
    -V, --version            Print version
