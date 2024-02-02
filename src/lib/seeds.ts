import storedWritable from "@efstajas/svelte-stored-writable";
import type { BaseUrl } from "@httpd-client";
import { z } from "zod";
import { api, httpdStore, type HttpdState } from "./httpd";
import { writable, derived, get } from "svelte/store";
import { config } from "./config";

const preferredSeedSchema = z
  .object({
    hostname: z.string(),
    port: z.number(),
    scheme: z.string(),
  })
  .optional();

const configuredPreferredSeeds = writable<BaseUrl[] | undefined>(undefined);
const storedPreferredSeed = storedWritable<BaseUrl | undefined>(
  "preferredSeed",
  preferredSeedSchema,
  undefined,
);

async function loadConfiguredPreferredSeeds() {
  if (get(httpdStore).state === "stopped") {
    configuredPreferredSeeds.set([]);
    return;
  }

  const profile = await api.profile.getProfile();

  let newValue = profile.config.preferredSeeds.map(seed => {
    const preferredSeedValue = seed?.split("@")[1];
    const preferredSeedOrigin = preferredSeedValue?.split(":")[0];

    return {
      hostname: preferredSeedOrigin,
      port: 443,
      scheme: "https",
    };
  });

  if (newValue.length === 0) {
    newValue = [config.fallbackPreferredSeed];
  }

  configuredPreferredSeeds.set(newValue);
}

export function initialize() {
  let previousHttpdState: HttpdState["state"] | undefined;

  httpdStore.subscribe(async v => {
    if (previousHttpdState === v.state) return;

    await loadConfiguredPreferredSeeds();

    previousHttpdState = v.state;
  });
}

export function selectPreferredSeed(seed: BaseUrl) {
  storedPreferredSeed.set(seed);
}

export const prefferedSeeds = derived(
  [configuredPreferredSeeds, storedPreferredSeed],
  ([configuredPreferredSeeds, storedPreferredSeed]) => {
    // Not loaded yet
    if (!configuredPreferredSeeds) return undefined;

    // No configured preferred seeds
    if (configuredPreferredSeeds.length === 0)
      return {
        selected: config.fallbackPreferredSeed,
        seeds: [config.fallbackPreferredSeed],
      };

    // No stored preferred seed
    if (!storedPreferredSeed)
      return {
        selected: configuredPreferredSeeds[0],
        seeds: configuredPreferredSeeds,
      };

    // Stored preferred seed not configured
    if (
      !configuredPreferredSeeds.some(
        seed => seed.hostname === storedPreferredSeed.hostname,
      )
    )
      return {
        selected: configuredPreferredSeeds[0],
        seeds: configuredPreferredSeeds,
      };

    return {
      selected: storedPreferredSeed,
      seeds: configuredPreferredSeeds,
    };
  },
);

export async function waitForLoad(): Promise<{
  selected: BaseUrl;
  seeds: BaseUrl[];
}> {
  if (!get(configuredPreferredSeeds)) {
    await new Promise<void>(resolve => {
      const unsubscribe = prefferedSeeds.subscribe(v => {
        if (v) {
          unsubscribe();
          resolve();
        }
      });
    });
  }

  const seeds = get(prefferedSeeds);
  if (!seeds) throw new Error("Preferred seed undefined after loading");

  return seeds;
}