use dam_core::Replacement;

pub fn redact(input: &str, replacements: &[Replacement]) -> String {
    let mut output = input.to_string();
    let mut sorted = replacements.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| b.span.start.cmp(&a.span.start));

    for replacement in sorted {
        if replacement.span.start <= output.len()
            && replacement.span.end <= output.len()
            && replacement.span.start <= replacement.span.end
        {
            output.replace_range(
                replacement.span.start..replacement.span.end,
                &replacement.text,
            );
        }
    }

    output
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
