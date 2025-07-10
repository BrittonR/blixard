#!/usr/bin/env python3

import os
import re
import subprocess

def get_files_with_missing_source():
    """Get list of files with missing source field errors"""
    try:
        result = subprocess.run(
            ["cargo", "check", "-p", "blixard-core", "--message-format=short"],
            capture_output=True,
            text=True,
            cwd="/home/brittonr/git/blixard"
        )
        
        files = set()
        for line in result.stderr.split('\n'):
            if 'missing field `source`' in line and 'BlixardError' in line:
                parts = line.split(':')
                if len(parts) >= 1:
                    file_path = parts[0]
                    if file_path.startswith('blixard-core/'):
                        files.add(file_path)
        
        return list(files)
    except Exception as e:
        print(f"Error getting files: {e}")
        return []

def fix_internal_errors_in_file(file_path):
    """Fix BlixardError::Internal missing source fields in a file"""
    try:
        with open(file_path, 'r') as f:
            content = f.read()
        
        # Pattern to match BlixardError::Internal { message: ... } without source
        # This matches the structure but allows for multiline formatting
        pattern = r'BlixardError::Internal\s*\{\s*message:\s*([^}]+?)\s*\}'
        
        def replacement(match):
            message_part = match.group(1).strip()
            # If message part ends with comma, keep it
            if message_part.endswith(','):
                return f'BlixardError::Internal {{ message: {message_part} source: None }}'
            else:
                return f'BlixardError::Internal {{ message: {message_part}, source: None }}'
        
        new_content = re.sub(pattern, replacement, content, flags=re.DOTALL)
        
        if new_content != content:
            with open(file_path, 'w') as f:
                f.write(new_content)
            print(f"Fixed: {file_path}")
            return True
        else:
            print(f"No changes needed: {file_path}")
            return False
            
    except Exception as e:
        print(f"Error processing {file_path}: {e}")
        return False

def main():
    print("Finding files with missing source field errors...")
    
    # Get files from cargo check output
    files = get_files_with_missing_source()
    
    # Convert to absolute paths
    base_dir = "/home/brittonr/git/blixard"
    files = [os.path.join(base_dir, f) for f in files]
    
    print(f"Found {len(files)} files to fix:")
    for f in files:
        print(f"  {f}")
    
    fixed_count = 0
    for file_path in files:
        if os.path.exists(file_path):
            if fix_internal_errors_in_file(file_path):
                fixed_count += 1
    
    print(f"\nFixed {fixed_count} files")

if __name__ == "__main__":
    main()