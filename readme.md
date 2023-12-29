# Rource

## What is it?
Rource is a tool to generate a Gource log file from a git repository or a directory containing git repositories.

If you don't know what Gource is you can check it out here: https://github.com/acaudwell/Gource

It's a tool to generate a cool visualisation from a log file, usually a Source Control system, like this:

[![Gource Video](gource.png)](https://youtu.be/NjUuAuBcoqs)

## How do I use it?

Give rource a path to a git repository or a directory containing repositories and it will generate a Gource log file.

You can control whether it should recursively search for repositories or not, and you can also specify which
repositories to include or exclude.

Rource also supports username aliases, so you can map various git usernames to cleaner, presentable names to display in
Gource.
It also supports shell expansion, so you can use `~` to refer to your home directory if you prefer.

### Examples

Scan a folder for git repositories recursive, apply two aliases and output to a file:

```shell
rource -p ~/source/my-github-org -r --alias "AdamLeyshon::WelshProgrammer" --alias "SomeOtherUsername::WelshProgrammer" -o output.txt
```

Scan a folder for git repositories recursive, apply two aliases and pipe directly to Gource:

```shell
rource -p ~/source/my-github-org -r | gource - --log-format custom
```

Generate log for a single repository and pipe directly to Gource:

```shell
rource -p ~/source/my-github-org/my-repo | gource - --log-format custom
```

Generate log for this repository and output to a file:

```shell
rource -p ./ -o output.txt
```

## How do I get it?

### Install from [crates.io](https://crates.io/)

It should be as simple as:

```shell
cargo install rource
```

### Building/Installing from source

```shell
git clone https://github.com/AdamLeyshon/rource.git 
cd rource

# You can either install it
cargo install --path .

# Or build it, the binary will be in ./target/release/rource
cargo build --release
```

## Available options

    Usage: rource [OPTIONS] --path <PATH>
    
    Options:
    -p, --path <PATH>        The path to the git repository/repositories
    -r, --recursive          Recursively search for repositories, by default all repositories in <PATH> will be included
    -i, --include <INCLUDE>  Used with recursive, only process these repositories, cannot be used with --exclude
    -e, --exclude <EXCLUDE>  Used with recursive, exclude these repositories from processing, cannot be used with --include
    -o, --output <OUTPUT>    Output file, defaults to stdout
    --alias <ALIAS>      Add an alias for a user, format is <GIT_USERNAME>::<GOURCE_USERNAME>, you can specify this option multiple times
    --no-logging         Disable logging, useful when piping directly tools that don't like stderr
    -h, --help               Print help
    -V, --version            Print version

## Tips

### GitHub Organisation

If you work in an organisation with many repositories on GitHub you use the GH CLI to clone them all locally and
then use rource to generate a log file for all of them.:

```shell
# Make a directory to clone all the repos into
mkdir ~/source/my-github-org
cd ~/source/my-github-org

# Login first if you haven't already
gh auth login

# Then clone all the repos, depending on the size, this may take a long time!
gh repo list <your-org-name> --limit 4000 | while read -r repo _; do
  gh repo clone "$repo" "$repo"                                                         
done

# Then generate the log file, plus an example to set an alias for a user and exclude an external repo 
rource -p ~/source/my-github-org -r --alias "GithubUsername::Friendly Name" -e "ADependencyLibrary" -o output.txt
```
