use std::collections::HashSet;

/// Extract variable names from SDL template
/// Variables are identified by ${VARIABLE_NAME} pattern
/// Variable names must be alphanumeric or underscore only
pub fn extract_variables(template: &str) -> HashSet<String> {
    let mut variables = HashSet::new();
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            if let Some(&'{') = chars.peek() {
                chars.next(); // consume '{'
                let mut var_name = String::new();

                // Collect variable name until we hit '}' or invalid character
                while let Some(&next_char) = chars.peek() {
                    if next_char == '}' {
                        chars.next(); // consume '}'
                        // Only add if variable name is valid (non-empty and alphanumeric/underscore)
                        if !var_name.is_empty() && var_name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            variables.insert(var_name);
                        }
                        break;
                    } else if next_char.is_alphanumeric() || next_char == '_' {
                        var_name.push(next_char);
                        chars.next();
                    } else {
                        // Invalid character in variable name, stop parsing this potential variable
                        break;
                    }
                }
            }
        }
    }

    variables
}

/// Validate that all variables in the template have corresponding defaults
pub fn validate_template_variables(
    template: &str,
    defaults: &std::collections::HashMap<String, String>,
) -> Result<(), Vec<String>> {
    let template_vars = extract_variables(template);
    let mut missing_defaults = Vec::new();

    for var in template_vars {
        if !defaults.contains_key(&var) {
            missing_defaults.push(var);
        }
    }

    if missing_defaults.is_empty() {
        Ok(())
    } else {
        missing_defaults.sort();
        Err(missing_defaults)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_extract_variables_simple() {
        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}"}"#;
        let vars = extract_variables(template);
        assert_eq!(vars.len(), 2);
        assert!(vars.contains("CPU"));
        assert!(vars.contains("MEMORY"));
    }

    #[test]
    fn test_extract_variables_nested() {
        let template = r#"{"resources": {"cpu": "${CPU}", "mem": "${MEMORY}", "storage": "${STORAGE}"}}"#;
        let vars = extract_variables(template);
        assert_eq!(vars.len(), 3);
        assert!(vars.contains("CPU"));
        assert!(vars.contains("MEMORY"));
        assert!(vars.contains("STORAGE"));
    }

    #[test]
    fn test_extract_variables_duplicate() {
        let template = r#"{"cpu": "${CPU}", "cpu2": "${CPU}"}"#;
        let vars = extract_variables(template);
        assert_eq!(vars.len(), 1);
        assert!(vars.contains("CPU"));
    }

    #[test]
    fn test_extract_variables_none() {
        let template = r#"{"cpu": "1.0", "memory": "512Mi"}"#;
        let vars = extract_variables(template);
        assert_eq!(vars.len(), 0);
    }

    #[test]
    fn test_extract_variables_malformed() {
        let template = r#"{"cpu": "${CPU", "memory": "$MEMORY}"}"#;
        let vars = extract_variables(template);
        // Should not extract malformed variables
        assert_eq!(vars.len(), 0);
    }

    #[test]
    fn test_validate_template_variables_valid() {
        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());

        assert!(validate_template_variables(template, &defaults).is_ok());
    }

    #[test]
    fn test_validate_template_variables_missing() {
        let template = r#"{"cpu": "${CPU}", "memory": "${MEMORY}", "storage": "${STORAGE}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());

        let result = validate_template_variables(template, &defaults);
        assert!(result.is_err());
        let missing = result.unwrap_err();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"MEMORY".to_string()));
        assert!(missing.contains(&"STORAGE".to_string()));
    }

    #[test]
    fn test_validate_template_variables_extra_defaults() {
        let template = r#"{"cpu": "${CPU}"}"#;
        let mut defaults = HashMap::new();
        defaults.insert("CPU".to_string(), "1.0".to_string());
        defaults.insert("MEMORY".to_string(), "512Mi".to_string());
        defaults.insert("STORAGE".to_string(), "1Gi".to_string());

        // Extra defaults are OK - they might be used in future template updates
        assert!(validate_template_variables(template, &defaults).is_ok());
    }
}
