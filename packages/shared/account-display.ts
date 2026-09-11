/** CDXC:AgentProviders 2026-09-10 DECISION: Hide emails keeps the first and last characters before @ and replaces every domain with the same six ・ characters followed by .•••, including email-shaped account names. Visible substitute characters replace the previous domain blur. */
export function maskAccountText(text: string): string {
  return text.replace(/([^\s@]+)@[^\s@]+/gu, (_, address: string) => {
    const characters = Array.from(address);
    return `${characters[0]}•••${characters.length > 1 ? characters.at(-1) : ''}@•••••.•••`;
  });
}
