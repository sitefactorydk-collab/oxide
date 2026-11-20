/** `src/appframe/sdk.ts`. */
export const SDK_FILE: string;
/** `src/appframe/oxide-sdk.d.ts`. */
export const DTS_FILE: string;
/** The SDK's types as the builder's model gets them (what `oxide-sdk.d.ts` must hold). */
export function sdkTypes(): Promise<string>;
