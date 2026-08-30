"""
Debug script to test the /process-transcript endpoint
"""
import requests
import json
import sys

def test_process_transcript(text="This is a test transcript"):
    """Test the process-transcript endpoint"""
    url = "http://localhost:5167/process-transcript"
    
    payload = {
        "text": text,
        "model": "claude",
        "model_name": "claude-3-5-sonnet-latest",
        "chunk_size": 5000,
        "overlap": 1000
    }
    
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json"
    }
    
    print(f"Sending request to {url}")
    print(f"Headers: {json.dumps(headers, indent=2)}")
    print(f"Payload: {json.dumps(payload, indent=2)}")
    
    try:
        response = requests.post(url, json=payload, headers=headers)
        print(f"Status Code: {response.status_code}")
        print(f"Response Headers: {json.dumps(dict(response.headers), indent=2)}")
        
        if response.status_code == 200:
            print(f"Response: {json.dumps(response.json(), indent=2)}")
        else:
            print(f"Error Response: {response.text}")
            
    except Exception as e:
        print(f"Error: {str(e)}")

if __name__ == "__main__":
    text = " ".join(sys.argv[1:]) if len(sys.argv) > 1 else "This is a test transcript"
    test_process_transcript(text)
