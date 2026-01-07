Use-Case:

- work on many projects, repos from various teams
- need personal project attached space which is persisted, versioned
- e.g. patched docker-compose.yml, custom Java test classes
- personal space cannot become part of official repo

Solution:
- file and directory overrides, when work starts swap personal data in, official repo files out
- when work ends revert, no traces of personal workspace left in project, original files restored

Requirements:
- gpg
- direnv

Use-Case:

- environment variables are not DRY
- often follow a hierarchy, e.g. global - company - region - stage

Solution:
- model it as hierarchical tree

Overall Use-Case
- Custom software development configuration and environment management
 
Requirements:
- None
