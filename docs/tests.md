Before any push I would run `scripts/tests.sh`, some notes before you do:
- It spins up a docker compose file for testing and then the tests point to that
  so it does not mess up your dev containers
- Defaults (like database url, aws config) is hard coded so don't 
  change the test docker compose file (soon i'll change that)
