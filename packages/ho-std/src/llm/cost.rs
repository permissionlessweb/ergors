/// Cost calculation helper for LLM providers
pub struct CostCalculator;

impl CostCalculator {
    /// Calculate cost based on token usage and provider pricing
    pub fn calculate_openai_cost(model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        let (input_rate, output_rate) = match model {
            "gpt-4" => (0.03, 0.06),
            "gpt-4-turbo" => (0.01, 0.03),
            "gpt-3.5-turbo" => (0.0015, 0.002),
            _ => (0.002, 0.002), // Default rate
        };

        (prompt_tokens as f64 / 1000.0) * input_rate
            + (completion_tokens as f64 / 1000.0) * output_rate
    }

    /// Calculate cost for Anthropic models
    pub fn calculate_anthropic_cost(
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> f64 {
        let (input_rate, output_rate) = match model {
            "claude-3-opus" => (0.015, 0.075),
            "claude-3-sonnet" => (0.003, 0.015),
            "claude-3-haiku" => (0.00025, 0.00125),
            _ => (0.003, 0.015), // Default to sonnet rates
        };

        (prompt_tokens as f64 / 1000.0) * input_rate
            + (completion_tokens as f64 / 1000.0) * output_rate
    }

    /// Generic cost calculation that routes to provider-specific calculators
    pub fn calculate_cost(
        provider: &str,
        model: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> f64 {
        match provider.to_lowercase().as_str() {
            "openai" => Self::calculate_openai_cost(model, prompt_tokens, completion_tokens),
            "anthropic" => Self::calculate_anthropic_cost(model, prompt_tokens, completion_tokens),
            "grok" => Self::calculate_openai_cost(model, prompt_tokens, completion_tokens), // Grok uses similar pricing
            _ => (prompt_tokens + completion_tokens) as f64 * 0.001 / 1000.0, // Generic fallback
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let cost = CostCalculator::calculate_openai_cost("gpt-4", 1000, 500);
        assert!(cost > 0.0);

        let cost2 = CostCalculator::calculate_cost("openai", "gpt-4", 1000, 500);
        assert_eq!(cost, cost2);
    }
}
