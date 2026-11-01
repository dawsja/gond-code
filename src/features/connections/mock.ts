// Mockups 18 and 18b's GitHub state, until Forge's github.status lands.
import type { DeviceCode, GithubStatus } from "@/lib/github";

const daysAgo = (d: number) => new Date(Date.now() - d * 86_400_000).toISOString();

export const MOCK_GITHUB: GithubStatus = {
  ghInstalled: true,
  ghVersion: "2.62.0",
  accounts: [
    {
      login: "sam",
      host: "github.com",
      active: true,
      scopes: ["repo", "read:org", "workflow", "gist"],
    },
    {
      login: "sam-work-with-a-rather-long-enterprise-login",
      host: "github.example-enterprise.com",
      active: false,
      scopes: ["repo", "read:org"],
      method: "token",
      addedAt: daysAgo(12),
    },
  ],
};

export const mockDeviceCode = async (): Promise<DeviceCode> => ({
  userCode: "4F2A-91C7",
  verificationUri: "https://github.com/login/device",
  expiresAt: new Date(Date.now() + 15 * 60_000).toISOString(),
});
