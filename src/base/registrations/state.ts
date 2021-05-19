import { derived, writable } from "svelte/store";

export enum State {
  Failed = -1,
  Idle,
  Committing,
  WaitingToRegister,
  Registering,
  Registered,
}

export const state = writable(State.Idle);

state.subscribe(s => {
  console.log("regiter.state", s);
});