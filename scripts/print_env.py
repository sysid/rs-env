import os

"""Script to test plugin
"""


def print_sorted_env_vars():
    """
    Print the environment variables in sorted order.
    """
    # Get environment variables
    env_vars = os.environ

    # Sort the environment variables by their names
    sorted_env_vars = sorted(env_vars.items())

    # Print each environment variable and its value
    for name, value in sorted_env_vars:
        if name.startswith("LESS_TERMCAP"):  # color output
            continue
        if name.startswith("BASH_FUNC"):
            continue
        if name.startswith("DIRENV"):
            continue
        if name.startswith("is_"):
            continue
        if name.startswith("_"):
            continue
        print(f"{name}: {value}")

        if name == "RUN_ENV" and value == "local":
            if name == "AWS_PROFILE":
                assert value == "xxx"
        elif name == "RUN_ENV" and value == "test":
            if name == "AWS_PROFILE":
                assert value == "e4m-test-userfull"


# Example usage
if __name__ == "__main__":
    print_sorted_env_vars()
