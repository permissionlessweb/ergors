from openai import OpenAI

client = OpenAI(
    api_key="EMPTY",
    base_url="http://localhost:30000/v1",
    timeout=3600
)

messages = [
    {
        "role": "user",
        "content": "Write a Python function that implements binary search on a sorted list. Include docstring and type hints."
    }
]

response = client.chat.completions.create(
    model="Qwen/Qwen3-Coder-480B-A35B-Instruct",
    messages=messages,
    max_tokens=2048,
    temperature=0.7
)

print(response.choices[0].message.content)