/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as commands from "../commands.js";
import type * as devMocks from "../devMocks.js";
import type * as http from "../http.js";
import type * as invites from "../invites.js";
import type * as lib_auth from "../lib/auth.js";
import type * as lib_linearApi from "../lib/linearApi.js";
import type * as lib_signatures from "../lib/signatures.js";
import type * as lib_slackApi from "../lib/slackApi.js";
import type * as lib_threadContext from "../lib/threadContext.js";
import type * as lib_tickets from "../lib/tickets.js";
import type * as linearIntake from "../linearIntake.js";
import type * as linearKeys from "../linearKeys.js";
import type * as slackFlow from "../slackFlow.js";
import type * as slackFlowReport from "../slackFlowReport.js";
import type * as slackFlowState from "../slackFlowState.js";
import type * as slackGithubIssue from "../slackGithubIssue.js";
import type * as slackIntake from "../slackIntake.js";
import type * as slackPost from "../slackPost.js";
import type * as slackRouting from "../slackRouting.js";
import type * as slackThreads from "../slackThreads.js";
import type * as teamFlow from "../teamFlow.js";
import type * as teamFlowSteps from "../teamFlowSteps.js";
import type * as teams from "../teams.js";
import type * as workCloudSessions from "../workCloudSessions.js";
import type * as workPage from "../workPage.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  commands: typeof commands;
  devMocks: typeof devMocks;
  http: typeof http;
  invites: typeof invites;
  "lib/auth": typeof lib_auth;
  "lib/linearApi": typeof lib_linearApi;
  "lib/signatures": typeof lib_signatures;
  "lib/slackApi": typeof lib_slackApi;
  "lib/threadContext": typeof lib_threadContext;
  "lib/tickets": typeof lib_tickets;
  linearIntake: typeof linearIntake;
  linearKeys: typeof linearKeys;
  slackFlow: typeof slackFlow;
  slackFlowReport: typeof slackFlowReport;
  slackFlowState: typeof slackFlowState;
  slackGithubIssue: typeof slackGithubIssue;
  slackIntake: typeof slackIntake;
  slackPost: typeof slackPost;
  slackRouting: typeof slackRouting;
  slackThreads: typeof slackThreads;
  teamFlow: typeof teamFlow;
  teamFlowSteps: typeof teamFlowSteps;
  teams: typeof teams;
  workCloudSessions: typeof workCloudSessions;
  workPage: typeof workPage;
}>;

/**
 * A utility for referencing Convex functions in your app's public API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = api.myModule.myFunction;
 * ```
 */
export declare const api: FilterApi<
  typeof fullApi,
  FunctionReference<any, "public">
>;

/**
 * A utility for referencing Convex functions in your app's internal API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = internal.myModule.myFunction;
 * ```
 */
export declare const internal: FilterApi<
  typeof fullApi,
  FunctionReference<any, "internal">
>;

export declare const components: {};
