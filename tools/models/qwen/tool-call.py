from openai import OpenAI

client = OpenAI(
    api_key="EMPTY",
    base_url="http://localhost:30000/v1",
    timeout=3600
)

# Define available tools
tools = [
    {
        "type": "function",
        "function": {
            "name": "execute_code",
            "description": "Execute Python code and return the result",
            "parameters": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The Python code to execute" # TODO: add as variable injected from rlm session
                    }
                },
                "required": ["code"]
            }
        }
    }
]

response = client.chat.completions.create(
    model="Qwen/Qwen3-Coder-30B-A3B-Instruct",
    messages=[
        {"role": "user", "content": "Calculate the factorial of 10 using Python"}
    ],
    tools=tools,
    temperature=0.7
)

# Check if the model wants to call a tool
if response.choices[0].message.tool_calls:
    tool_call = response.choices[0].message.tool_calls[0]
    print(f"Tool: {tool_call.function.name}")
    print(f"Arguments: {tool_call.function.arguments}")
else:
    # Model may return tool call in content format
    print(response.choices[0].message.content)