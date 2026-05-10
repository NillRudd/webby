# Skill: Refactor Safely

Use this when restructuring Webby without changing behavior.

## Steps

1. Identify current behavior that must remain unchanged.
2. Add tests first if behavior is untested.
3. Move/rename code in small steps.
4. Keep public APIs stable unless the task is to change them.
5. Run tests after refactor.
6. Explain what moved and what did not change.

## Good refactors

- split URL resolver from app state
- move DOM structs into `dom.rs`
- move display command definitions into `render.rs`

## Bad refactors

- changing parser behavior while moving files
- introducing traits/generics without a concrete need
- broad cleanup mixed with feature work
