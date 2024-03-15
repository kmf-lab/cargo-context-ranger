use std::error::Error;
use structopt::StructOpt;
use regex::Regex;
use walkdir::WalkDir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use clipboard::{ClipboardContext, ClipboardProvider};


#[derive(StructOpt)]
struct Cli {
    /// The folder location of the Rust project
    #[structopt(short ="p", long="path", parse(from_os_str), default_value = ".")]
    folder_location: PathBuf,

    /// Full module path to the target function (e.g., my_mod::my_sub_mod::my_fn)
    #[structopt(short="f", long="function", default_value = "main")]
    function_path: String,

    ///optional window size in K 1024 chars
    #[structopt(short = "w", long="window", default_value = "32")] //default is 32K
    window_size_k: usize,

}

//TODO: we need N levels of scanning for better order placement
//TODO: we need java support
//TODO: we need a boolean argument for clipboard use (also clipboard needs cleanup)


fn extract_function_body(content: &str, function_name: &str) -> String {
    let function_pattern = Regex::new(&format!(r"fn {}\([^\)]*\) \{{", regex::escape(function_name))).unwrap();
    if let Some(mat) = function_pattern.find(content) {
        return content[mat.start()..].to_string(); // Simplification: Assumes function body ends at the end of the file
    }
    "".to_string()
}

fn replace_blocks_not_calling_target_function(content: &str, def_pos: Option<usize>, target_function_caller_pattern: &Regex ) -> String {

    let mut brace_depth = 0;
    let mut last_index = 0;
    let mut result = String::new();
    let mut keep_safe = false;

    for (index, char) in content.char_indices() {
        match char {
            '{' => {
                brace_depth += 1;
            },
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    let block_content = &content[last_index..index+1];

                    // if this is the definition of the function or we call it then keep
                    if keep_safe || target_function_caller_pattern.is_match(block_content) {
                        // Keep the original block
                        result.push_str(block_content);
                        keep_safe = false;
                    } else {
                        // Replace the block content if it doesn't call the target function
                        result.push_str("{...}");
                    }

                    last_index = index + 1;
                }
            },
            _ => {}
        }

        // Append content outside of blocks
        if brace_depth == 0 && index >= last_index {
            result.push(char);
            last_index = index + 1;
            if let Some(def_pos) = def_pos {
                if index==def_pos {
                    keep_safe = true;
                }
            }
        }
    }

    // Append any remaining content after the last block
    result.push_str(&content[last_index..]);

    result
}


/// finds all files defining this function which may be in the right module path.
/// this can include false positives for corner cases but should not miss the case we want.
/// extra context and 'examples' would actually be helpful to the LLM.
/// Returns all rs files with (Ordering, Path, TrimmedBody, Optional Full Body)
fn find_source_files(folder_location: &Path, function_path: &str) -> Vec<(u8, PathBuf, String, Option<String>)> {
    let parts: Vec<&str> = function_path.split("::").collect();
    let function_name = if let Some(last) = parts.last() {     last
                                                  } else {     "main" //look for main if function_path not provided
    };
    let module_parts = &parts[..parts.len() - 1]; // Exclude the function name part


    let target_function_def_pattern = Regex::new(&format!(r"fn\s+{}\s*[<(]", function_name)).unwrap();

    let target_function_caller_pattern = Regex::new(&format!(r"[.:]{}\s*[:<\(]", function_name)).unwrap();


    let all_target_module_patterns: Vec<(String, Regex)> = module_parts
        .iter()
        .map(|mod_name| (mod_name.to_string(), Regex::new(&format!(r"\bmod\s+{}\s*;", mod_name)).unwrap() ))
        .collect();

    WalkDir::new(folder_location).into_iter()
        .filter_map(Result::ok) // calls the ok method on each Result in the iterator, discarding any Err values
        .filter_map(|entry|
            //either it ends in the right extension for source files or we return false
            if entry.path().extension().map_or(false, |ext| ext == "rs") {
                let content = fs::read_to_string(entry.path()).expect("Unable to read file"); //should not happen as we checked ok

                //to save prompt space we will trim every line of this file before using it
                //at the same time we also do not put blank lines back in, confirmed this saves tokens
                let content = content.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<&str>>()
                    .join("\n");

                // Check if the file contains our target function pattern
                if let Some(thing) = target_function_def_pattern.find(&content) {
                    // Check for path match or module declaration match, our function must be
                    // in a mod defined here or in a mod in the path, all mods must be found
                    // due to mod ordering this may allow false positives and that is ok
                    if all_target_module_patterns.iter().all(|(mod_name,re)|
                            re.is_match(&content) || entry.path().display().to_string().contains(mod_name)
                        ) {

                        println!("Function '{}' found in file: {:?} at: {}", function_name, entry.path(), thing.start());
                        let summary = replace_blocks_not_calling_target_function(&content, Some(thing.start()), &target_function_caller_pattern);
                        Some((0,entry.clone().into_path(),summary,Some(content)))
                    } else {
                        //same function name but not in the right modules, could be a great example
                        //this is a lower priority than our immediate context
                        Some((3,entry.into_path(),content,None))
                    }
                } else {
                    //somewhere on this page we found a call to our function so this is a higher priority.
                    if target_function_caller_pattern.is_match(&content) {
                        let summary = replace_blocks_not_calling_target_function(&content, None, &target_function_caller_pattern);
                        Some((1,entry.into_path(),summary,Some(content)))
                    } else {
                        //does not contain and does not call our target function so this is the lowest priority
                        Some((4,entry.into_path(),content,None))
                    }
                }
            } else {
                None
            }
    ).collect()
}

fn main() {

    let args = Cli::from_args();
    let window_size_bytes = args.window_size_k * 1024;

    //collect all the results into a single string
    let mut output_for_the_llm = String::new();

    let meta = true; //TODO: we may want a command line to turn this off in some cases.
    if meta {  //other data beyond the source which may be helpful
        output_for_the_llm.push_str(" For the purposes of answering this question you are a helpful principal software engineer with a formal yet optimistic attitude. Here is the context available to complete the task.\n");
        output_for_the_llm.push_str( &format!("Keep your focus on the function: {:?}", &args.function_path));

        let rustc_output = Command::new("rustc")
            .arg("--version")
            .output()
            .expect("Failed to retrieve Rust compiler version");

        let rustc_version = String::from_utf8_lossy(&rustc_output.stdout);

        println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version.trim());
        output_for_the_llm.push_str(&format!("You are running on: {}\n", std::env::consts::OS));
        output_for_the_llm.push_str(&format!("cargo:rustc-env=RUSTC_VERSION={}\n", rustc_version.trim()));
    }


    let extra_content_size =  output_for_the_llm.len();

    let all_files = gather_sorted_vec_of_bodies(&args, &mut output_for_the_llm);
    join_all_bodies(window_size_bytes, &mut output_for_the_llm, extra_content_size, &all_files, 2);

    //trim the results to the window size
    if output_for_the_llm.len() > window_size_bytes {
        output_for_the_llm = output_for_the_llm.chars().take(window_size_bytes).collect();
    }
    println!("{}", &output_for_the_llm);
    //Kb written
    println!("{}KBs of content", output_for_the_llm.len() / 1024);

    // At the very end, after constructing the result string:

    //write result to my clipboard, TODO: this is messy and needs a boolean to enable
    //we just try both clipboards and ignore the errors, a bit smelly

    { //wayland clipboard if you have it
        use wl_clipboard_rs::copy::{MimeType, Options, Source};
        let opts = Options::new();
        let _ = opts.copy(Source::Bytes(output_for_the_llm.clone().into_bytes().into()), MimeType::Autodetect);
    }

    //other x11 clipboard if you have it
    let ctx: Result<ClipboardContext, Box<dyn std::error::Error>> = ClipboardProvider::new();
    match ctx {
        Ok(mut ctx) => {
            // Here, `set_contents` is clearly operating on a ClipboardContext
            match ctx.set_contents(output_for_the_llm.clone()) {
                Ok(a) => {println!("Result has been copied to the clipboard. {:?}",a);
                          //we must read the clip board back to ensure it worked
                          ctx.get_contents().map(|s| println!("Clipboard contents: {}KB", s.len()/1024)).ok();
                },
                Err(e) => eprintln!("Failed to copy to the clipboard: {}", e),
            }
        },
        Err(e) => { } ,
    }


}

fn gather_sorted_vec_of_bodies(args: &Cli, result: &mut String) -> Vec<(u8, PathBuf, String, Option<String>)> {
    let source_folder = args.folder_location.join("src");
    let mut all_files = find_source_files(&source_folder, &args.function_path);
    //add the cargo file as priority 2 which is not found in the src folder
    let assumed_cargo_path = args.folder_location.join("Cargo.toml");
    if let Ok(body) = fs::read_to_string(&assumed_cargo_path) {
        all_files.push((2, assumed_cargo_path, body, None));
    } else {
        result.push_str("Unable to find Cargo.toml, this is probably a new project\n");
    }
    all_files.sort();//must be sorted so the high priority comes first
    all_files
}

fn join_all_bodies(window_size_bytes: usize, result: &mut String, extra_content_size: usize, all_files: &Vec<(u8, PathBuf, String, Option<String>)>, goal: u8) {

    //if it turns out our context window is large we can rollback our source summary and use full files.
    let draft_counts: usize = all_files.iter()
        .filter(|(order, _, _, _)| *order <= goal)
        .map(|(_, _, content, optional)| if let Some(op) = optional { op.len() } else { content.len() })
        .sum();
    let use_optional_full_unmodified_source_file = draft_counts + extra_content_size <= window_size_bytes;

    for (_, path, content, optional) in all_files {
        result.push_str(&format!("\n\n\n////// Top of File: {} //////\n\n", path.display()));
        if use_optional_full_unmodified_source_file {
            if let Some(op) = optional {
                result.push_str(&op);
            } else {
                result.push_str(&content);
            }
        } else {
            result.push_str(&content);
        }
        result.push_str(&format!("\n\n////// End of File: {} //////\n\n\n", path.display()));
    }
}
