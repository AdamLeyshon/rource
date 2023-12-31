use clap::Parser;

#[derive(Parser)]
#[command(
    name = "Rource",
    author = "Adam Leyshon",
    version = "0.3.0",
    about,
    long_about = "
Rource! A tool to convert git logs into a format that can be used by Gource.

WARNING: When used with large quantities of repositories or a repository with many commits, Rource can generate very large files.
In some cases you may need to use the --use-merge-sort option, please read the documentation for this option for more info.
"
)]
pub struct ClapArguments {
    #[arg(short, long, help = "The path to the git repository/repositories")]
    pub path: String,

    #[arg(
        short,
        long,
        help = "Recursively search for repositories, by default all repositories in <PATH> will be included"
    )]
    pub recursive: bool,

    #[arg(
        requires = "recursive",
        short,
        long,
        help = "Used with recursive, only process these repositories, cannot be used with --exclude"
    )]
    pub include: Vec<String>,

    #[arg(
        requires = "recursive",
        conflicts_with = "include",
        short,
        long,
        help = "Used with recursive, exclude these repositories from processing, cannot be used with --include"
    )]
    pub exclude: Vec<String>,

    #[arg(short, long, help = "Output file, defaults to stdout")]
    pub output: Option<String>,

    #[arg(
        long,
        short = 'a',
        help = "Add an alias for a user",
        long_help = "Add an alias for a user, the format is <USERNAME>::<REPLACEMENT>,
If a username contains pipes (|), they are automatically be replaced with '#' before aliases are applied, 
If you want to alias 'Some|User', your alias should be 'Some#User::SomeUser'.
You can specify this option multiple times"
    )]
    pub alias: Vec<String>,

    #[arg(
        long,
        short = 'm',
        help = "Use a disk-backed Merge Sort",
        long_help = "Use Merge Sort, required when processing large quantities of commits.
Be aware that when using the merge sort, you will need at least 3x the size of the final log file in free disk space.
For example, when used against the Rust repository, the final output is 64GB but the temporary space needed is upto 192GB.
Please also read the documentation for --sort-chunk-size and --temp-file-location for additional controls"
    )]
    pub use_merge_sort: bool,

    #[arg(
        long,
        help = "Merge sort chunk size in MB, min: 64, default: 4096",
        long_help = "Chunk size in Megabytes (Min: 64 MB), Merge sort will try to limit RAM usage to this amount, \
        however it is not a hard limit and should be viewed as a hint, by default it will use 4 GB. \
        Depending on the number of commits, more RAM will help speed up the sort/merge phase",
        requires = "use_merge_sort"
    )]
    pub sort_chunk_size: Option<u64>,

    #[arg(
        long,
        short,
        help = "Location to use for temporary merge-sort files",
        long_help = "Location to store temporary files, by default this will randomly named \
         directory in the current working path, if the program is interrupted you may \
         need to delete this directory manually",
        requires = "use_merge_sort"
    )]
    pub temp_file_location: Option<String>,

    #[arg(
        long,
        short = 'z',
        long_help = "Commits with a changeset larger than this will be filtered out, \
        this is useful for ignoring commits that are likely to be merges, tags or CI/CD commits",
        help = "Maximum changeset size per commit, default is unlimited"
    )]
    pub max_changeset_size: Option<usize>,
}
