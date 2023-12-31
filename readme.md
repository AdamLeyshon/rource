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

    Rource! A tool to convert git logs into a format that can be used by Gource.
    
    WARNING: When used with large quantities of repositories or a repository with many commits, Rource can generate very large files.
    In some cases you may need to use the --use-merge-sort option, please read the documentation for this option for more info.

    Usage: rource [OPTIONS] --path <PATH>
    
    Options:
        -p, --path <PATH>
            The path to the git repository/repositories
        
        -r, --recursive
            Recursively search for repositories, by default all repositories in <PATH> will be included
        
        -i, --include <INCLUDE>
            Used with recursive, only process these repositories, cannot be used with --exclude
        
        -e, --exclude <EXCLUDE>
            Used with recursive, exclude these repositories from processing, cannot be used with --include
        
        -o, --output <OUTPUT>
            Output file, writes to stdout if not specified
        
        -a, --alias <ALIAS>
            Add an alias for a user, the format is <USERNAME>::<REPLACEMENT>,
            If a username contains pipes (|), they are automatically be replaced with '#' before aliases are applied,
            If you want to alias 'Some|User', your alias should be 'Some#User::SomeUser'.
            You can specify this option multiple times
        
        -m, --use-merge-sort
            Use Merge Sort, required when processing large quantities of commits.
            Be aware that when using the merge sort, you will need at least 3x the size of the final log file in free disk space.
            For example, when used against the Rust repository, the final output is 64GB but the temporary space needed is upto 192GB.
            Please also read the documentation for --sort-chunk-size and --temp-file-location for additional controls
        
        --sort-chunk-size <SORT_CHUNK_SIZE>
            Chunk size in Megabytes (Min: 64 MB), Merge sort will try to limit RAM usage to this amount, however it is not a hard limit and should be viewed as a hint, by default it will use 4 GB. Depending on the number of commits, more RAM will help speed up the sort/merge phase
        
        -t, --temp-file-location <TEMP_FILE_LOCATION>
            Location to store temporary files, by default this will randomly named directory in the current working path, if the program is interrupted you may need to delete this directory manually
        
        -z, --max-changeset-size <MAX_CHANGESET_SIZE>
            Commits with a changeset larger than this will be filtered out, this is useful for ignoring commits that are likely to be merges, tags or CI/CD commits
        
        -h, --help
            Print help (see a summary with '-h')
        
        -V, --version
            Print version


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
