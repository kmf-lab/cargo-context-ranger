use structopt::StructOpt;
use regex::Regex;
use walkdir::WalkDir;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(StructOpt)]
struct Cli {
    /// The folder location of the Rust project
    #[structopt(short ="p", long="path", parse(from_os_str), default_value = ".")]
    folder_location: PathBuf,

    /// Full module path to the target function (e.g., my_mod::my_sub_mod::my_fn)
    #[structopt(short="f", long="function", default_value = "main")]
    function_path: String,

    ///optional window size in K 1024 chars
    #[structopt(short = "w", long="window", default_value = "16")] //default is 16K
    window_size_k: usize,

}


fn extract_function_body(content: &str, function_name: &str) -> String {
    let function_pattern = Regex::new(&format!(r"fn {}\([^\)]*\) \{{", regex::escape(function_name))).unwrap();
    if let Some(mat) = function_pattern.find(content) {
        return content[mat.start()..].to_string(); // Simplification: Assumes function body ends at the end of the file
    }
    "".to_string()
}


fn modify_function_bodies(file_path: &PathBuf, function_name: &str) -> String {
    let content = fs::read_to_string(file_path).expect("Unable to read file");
    let function_pattern = Regex::new(r"fn\s+(\w+)\s*\((.*?)\)\s*\{([\s\S]*?)\}").expect("bad regex");
    let mut modified_content = content.clone();

    // Extract all functions and their bodies
    let mut function_calls = Vec::new();
    for cap in function_pattern.captures_iter(&content) {
        let name = cap.get(1).unwrap().as_str();
        let body = cap.get(3).unwrap().as_str();

        // Check if the function body calls the target function
        if body.contains(&format!("{}(", function_name)) || body.contains(&format!("{} (", function_name)) {
            function_calls.push(name);
        }
    }

    // Replace bodies of functions that don't call the target function and aren't called by it
    for cap in function_pattern.captures_iter(&content) {
        let name = cap.get(1).unwrap().as_str();
        let whole_match = cap.get(0).unwrap().as_str();

        if !function_calls.contains(&name) && name != function_name {
            let replacement = format!("fn {}(...) {{ ... }}", name);
            modified_content = modified_content.replace(whole_match, &replacement);
        }
    }
    modified_content
}


/// finds all files defining this function which may be in the right module path.
/// this can include false positives for corner cases but should not miss the case we want.
/// extra context and 'examples' would actually be helpful to the LLM.
/// Returns all rs files with (Ordering, Path, TrimmedBody, Optional Full Body)
fn find_files(folder_location: &Path, function_path: &str) -> Vec<(u8,PathBuf,String,Option<String>)> {
    let parts: Vec<&str> = function_path.split("::").collect();
    let function_name = if let Some(last) = parts.last() {
        last
    } else {
        "main" //look for main if function not provided
    };

    let module_parts = &parts[..parts.len() - 1]; // Exclude the function name part
    let function_pattern = Regex::new(&format!(r"fn\s+{}\s*\(", function_name)).unwrap();

    // Prepare regex patterns for each module part to check for `mod XX;` declarations
    let module_decl_patterns: Vec<Regex> = module_parts
        .iter()
        .map(|mod_name| Regex::new(&format!(r"\bmod\s+{}\s*;", mod_name)).unwrap())
        .collect();

    WalkDir::new(folder_location).into_iter().filter_map(Result::ok).filter_map(|entry|
        if entry.path().extension().map_or(false, |ext| ext == "rs") {
            let content = fs::read_to_string(entry.path()).expect("Unable to read file");
            // Check if the file contains the function pattern
            if function_pattern.is_match(&content) {
                // Check for path match or module declaration match
                if module_parts.iter().all(|&mod_name| entry.path().display().to_string().contains(mod_name)) ||
                    module_decl_patterns.iter().any(|re| re.is_match(&content)) {
                    println!("Function '{}' found in file: {:?}", function_name, entry.path());
                    Some((0,entry.clone().into_path(),modify_function_bodies(&entry.into_path(), function_name),Some(content)))
                } else {
                    //same function name but not in the right modules, could be a great example
                    //this is a lower priority than our immediate context
                    Some((3,entry.into_path(),content,None))
                }
            } else {
                if content.contains(&format!("{}(", function_name)) || content.contains(&format!("{} (", function_name)) {
                    Some((1,entry.into_path(),content,None))
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
    let mut result = String::new();

    let meta = true;
    if meta {
        result.push_str(" For the purposes of answering this question you are a helpful principal software engineer with a formal yet optimistic attitude. Here is the context available to complete the task.\n");

        let rustc_output = Command::new("rustc")
            .arg("--version")
            .output()
            .expect("Failed to retrieve Rust compiler version");

        let rustc_version = String::from_utf8_lossy(&rustc_output.stdout);

        println!("cargo:rustc-env=RUSTC_VERSION={}", rustc_version.trim());
        result.push_str(&format!("You are running on: {}\n", std::env::consts::OS));
        result.push_str(&format!("cargo:rustc-env=RUSTC_VERSION={}\n", rustc_version.trim()));

    }

    let extra_content_size =  result.len();

    let mut all_files = find_files(&args.folder_location, &args.function_path);
    //add the cargo file as priority 2
    let assumed_cargo_path = args.folder_location.join("Cargo.toml");
    if let Ok(body) = fs::read_to_string(&assumed_cargo_path) {
        all_files.push((2,assumed_cargo_path,body,None));
    } else {
        result.push_str("Unable to find Cargo.toml, this is probably a new project\n");
    }


    all_files.sort();
    //if the total bytes is small enough up all 2's we can use the full file we will
    let draft_counts:usize = all_files.iter()
                     .filter(|(order,_,_,_)| *order <= 2)
                     .map(|(_,_,content,optional)| if let Some(op) = optional {op.len()} else {content.len()}  )
                     .sum();
    let use_option = draft_counts+extra_content_size <= window_size_bytes;



    for (order,path,content,optional) in all_files {
        if use_option {
            result.push_str(&format!("// ** // File: {}\n", path.display()));

            if let Some(op) = optional {
                result.push_str(&op);
            } else {
                result.push_str(&content);
            }
        } else {
            result.push_str(&content);
        }
    }

    //trim the results to the window size
    if result.len() > window_size_bytes {
        result = result.chars().take(window_size_bytes).collect();
    }
    println!("{}", result)

}
