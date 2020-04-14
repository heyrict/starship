use ansi_term::ANSIStrings;
use clap::ArgMatches;
use rayon::prelude::*;
use std::fmt::{self, Debug, Write as FmtWrite};
use std::io::{self, Write};
use unicode_width::UnicodeWidthChar;

use crate::configs::PROMPT_ORDER;
use crate::context::{Context, Shell};
use crate::formatter::StringFormatter;
use crate::messages;
use crate::module::Module;
use crate::module::ALL_MODULES;
use crate::modules;
use crate::segment::Segment;

pub fn prompt(args: ArgMatches) {
    let context = Context::new(args);
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    write!(handle, "{}", get_prompt(context)).unwrap();
}

pub fn get_prompt(context: Context) -> String {
    let config = context.config.get_root_config();
    let mut buf = String::new();

    // A workaround for a fish bug (see #739,#279). Applying it to all shells
    // breaks things (see #808,#824,#834). Should only be printed in fish.
    if let Shell::Fish = context.shell {
        buf.push_str("\x1b[J"); // An ASCII control code to clear screen
    }

    let formatter = if let Ok(formatter) = StringFormatter::new(config.format) {
        formatter
    } else {
        log::error!("Error parsing `format`");
        buf.push_str(">");
        return buf;
    };
    let modules: Vec<String> = formatter
        .get_variables()
        .into_iter()
        .map(|var| var.to_string())
        .collect();
    let formatter = formatter.map_variables_to_segments(|module| {
        // Make $all display all modules
        if module == "all" {
            Some(
                PROMPT_ORDER
                    .par_iter()
                    .flat_map(|module| match *module {
                        "\n" => {
                            let mut line_break = Segment::new("line_break");
                            line_break.set_value("\n");
                            Some(vec![line_break])
                        }
                        _ => Some(
                            handle_module(module, &context, &modules)
                                .into_iter()
                                .flat_map(|module| module.segments)
                                .collect::<Vec<Segment>>(),
                        ),
                    })
                    .flatten()
                    .collect::<Vec<_>>(),
            )
        } else if context.is_module_disabled_in_config(&module) {
            None
        } else {
            // Get segments from module
            Some(
                handle_module(module, &context, &modules)
                    .into_iter()
                    .flat_map(|module| module.segments)
                    .collect::<Vec<Segment>>(),
            )
        }
    });

    // Adds messages if `prompt_order` and `add_newline` found in the config
    if let Some(config) = &context.config.config {
        let table = config.as_table().unwrap();
        if table.contains_key("prompt_order") || table.contains_key("add_newline") {
            messages::add(messages::messages::DEPRECATED_USE_FORMAT);
        }
    };

    // Inserts messages before all segments if there are some messages
    let mut segments = messages::get_segments(&config.messages);
    segments.extend(formatter.parse(None));

    // Update viewed messages
    if let Err(error) = messages::update_viewed_hash() {
        log::warn!("Error updating viewed messages: {}", error);
    };

    // Creates a root module and prints it.
    let mut root_module = Module::new("Starship Root", "The root module", None);
    root_module.get_prefix().set_value("");
    root_module.get_suffix().set_value("");
    root_module.set_segments(segments);

    let module_strings = root_module.ansi_strings_for_shell(context.shell.clone());
    write!(buf, "{}", ANSIStrings(&module_strings)).unwrap();

    buf
}

pub fn module(module_name: &str, args: ArgMatches) {
    let context = Context::new(args);
    let module = get_module(module_name, context).unwrap_or_default();
    print!("{}", module);
}

pub fn get_module(module_name: &str, context: Context) -> Option<String> {
    modules::handle(module_name, &context).map(|m| m.to_string())
}

pub fn explain(args: ArgMatches) {
    let context = Context::new(args);

    struct ModuleInfo {
        value: String,
        value_len: usize,
        desc: String,
    }

    let dont_print = vec!["character"];

    let modules = compute_modules(&context)
        .into_iter()
        .filter(|module| !dont_print.contains(&module.get_name().as_str()))
        .map(|module| {
            let ansi_strings = module.ansi_strings();
            let value = module.get_segments().join("");
            ModuleInfo {
                value: ansi_term::ANSIStrings(&ansi_strings[1..ansi_strings.len() - 1]).to_string(),
                value_len: value.chars().count() + count_wide_chars(&value),
                desc: module.get_description().to_owned(),
            }
        })
        .collect::<Vec<ModuleInfo>>();

    let mut max_ansi_module_width = 0;
    let mut max_module_width = 0;

    for info in &modules {
        max_ansi_module_width = std::cmp::max(
            max_ansi_module_width,
            info.value.chars().count() + count_wide_chars(&info.value),
        );
        max_module_width = std::cmp::max(max_module_width, info.value_len);
    }

    let desc_width = term_size::dimensions()
        .map(|(w, _)| w)
        .map(|width| width - std::cmp::min(width, max_ansi_module_width));

    println!("\n Here's a breakdown of your prompt:");
    for info in modules {
        let wide_chars = count_wide_chars(&info.value);

        if let Some(desc_width) = desc_width {
            let wrapped = textwrap::fill(&info.desc, desc_width);
            let mut lines = wrapped.split('\n');
            println!(
                " {:width$}  -  {}",
                info.value,
                lines.next().unwrap(),
                width = max_ansi_module_width - wide_chars
            );

            for line in lines {
                println!("{}{}", " ".repeat(max_module_width + 6), line.trim());
            }
        } else {
            println!(
                " {:width$}  -  {}",
                info.value,
                info.desc,
                width = max_ansi_module_width - wide_chars
            );
        };
    }
}

fn compute_modules<'a>(context: &'a Context) -> Vec<Module<'a>> {
    let mut prompt_order: Vec<Module<'a>> = Vec::new();

    let config = context.config.get_root_config();
    let formatter = if let Ok(formatter) = StringFormatter::new(config.format) {
        formatter
    } else {
        log::error!("Error parsing `format`");
        return Vec::new();
    };
    let modules = formatter.get_variables().clone();

    for module in &modules {
        let modules = handle_module(module, &context, &modules);
        prompt_order.extend(modules.into_iter());
    }

    prompt_order
}

fn handle_module<'a, T>(module: &str, context: &'a Context, module_list: &[T]) -> Vec<Module<'a>>
where
    T: AsRef<str>,
{
    struct DebugCustomModules<'tmp>(&'tmp toml::value::Table);

    impl Debug for DebugCustomModules<'_> {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.debug_list().entries(self.0.keys()).finish()
        }
    }

    let mut modules: Vec<Option<Module>> = Vec::new();

    if ALL_MODULES.contains(&module) {
        // Write out a module if it isn't disabled
        if !context.is_module_disabled_in_config(module) {
            modules.push(modules::handle(module, &context));
        }
    } else if module == "custom" {
        // Write out all custom modules, except for those that are explicitly set
        if let Some(custom_modules) = context.config.get_custom_modules() {
            let custom_modules = custom_modules
                .iter()
                .map(|(custom_module, config)| {
                    if should_add_implicit_custom_module(custom_module, config, &module_list) {
                        modules::custom::module(custom_module, &context)
                    } else {
                        None
                    }
                })
                .collect::<Vec<Option<Module<'a>>>>();
            modules.extend(custom_modules)
        }
    } else if module.starts_with("custom.") {
        // Write out a custom module if it isn't disabled (and it exists...)
        match context.is_custom_module_disabled_in_config(&module[7..]) {
            Some(true) => (), // Module is disabled, we don't add it to the prompt
            Some(false) => modules.push(modules::custom::module(&module[7..], &context)),
            None => match context.config.get_custom_modules() {
                Some(modules) => log::debug!(
                    "prompt_order contains custom module \"{}\", but no configuration was provided. Configuration for the following modules were provided: {:?}",
                    module,
                    DebugCustomModules(modules),
                    ),
                None => log::debug!(
                    "prompt_order contains custom module \"{}\", but no configuration was provided.",
                    module,
                    ),
            },
        }
    } else {
        log::debug!(
            "Expected prompt_order to contain value from {:?}. Instead received {}",
            ALL_MODULES,
            module,
        );
    }

    modules.into_iter().flatten().collect()
}

fn should_add_implicit_custom_module<T>(
    custom_module: &str,
    config: &toml::Value,
    config_prompt_order: &[T],
) -> bool
where
    T: AsRef<str>,
{
    let is_explicitly_specified = config_prompt_order.iter().any(|x| {
        let x: &str = x.as_ref();
        x.len() == 7 + custom_module.len() && &x[..7] == "custom." && &x[7..] == custom_module
    });

    if is_explicitly_specified {
        // The module is already specified explicitly, so we skip it
        return false;
    }

    let false_value = toml::Value::Boolean(false);

    !config
        .get("disabled")
        .unwrap_or(&false_value)
        .as_bool()
        .unwrap_or(false)
}

fn count_wide_chars(value: &str) -> usize {
    value.chars().filter(|c| c.width().unwrap_or(0) > 1).count()
}
