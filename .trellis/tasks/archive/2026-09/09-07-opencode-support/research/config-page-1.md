Providers | OpenCode  
[Skip to content](#_top)

[![](/docs/_astro/logo-dark.DOStV66V.svg) ![](/docs/_astro/logo-light.B0yzR0O5.svg)  OpenCode](/docs/)

[app.header.home](/)[app.header.docs](/docs/)

Search  `CtrlK`   

Cancel

   

* [Intro](/docs/)
* [Config](/docs/config/)
* [Providers](/docs/providers/)
* [Network](/docs/network/)
* [Enterprise](/docs/enterprise/)
* [Troubleshooting](/docs/troubleshooting/)
* [Windows](/docs/windows-wsl)
* Usage

  + [Go](/docs/go/)
  + [TUI](/docs/tui/)
  + [CLI](/docs/cli/)
  + [Web](/docs/web/)
  + [IDE](/docs/ide/)
  + [Zen](/docs/zen/)
  + [Share](/docs/share/)
  + [GitHub](/docs/github/)
  + [GitLab](/docs/gitlab/)
* Configure

  + [Tools](/docs/tools/)
  + [Rules](/docs/rules/)
  + [Agents](/docs/agents/)
  + [Models](/docs/models/)
  + [Themes](/docs/themes/)
  + [Keybinds](/docs/keybinds/)
  + [Commands](/docs/commands/)
  + [Formatters](/docs/formatters/)
  + [Permissions](/docs/permissions/)
  + [Policies](/docs/policies/)
  + [LSP Servers](/docs/lsp/)
  + [MCP servers](/docs/mcp-servers/)
  + [ACP Support](/docs/acp/)
  + [Agent Skills](/docs/skills/)
  + [References](/docs/references/)
  + [Custom Tools](/docs/custom-tools/)
* Develop

  + [SDK](/docs/sdk/)
  + [Server](/docs/server/)
  + [Plugins](/docs/plugins/)
  + [Ecosystem](/docs/ecosystem/)

[GitHub](https://github.com/anomalyco/opencode)[Discord](https://opencode.ai/discord)

  Select theme  DarkLightAuto      Select language  EnglishالعربيةBosanskiDanskDeutschEspañolFrançaisItaliano日本語한국어Norsk BokmålPolskiPortuguês (Brasil)РусскийไทยTürkçe简体中文繁體中文

On this page

* [Overview](#_top) 
  + [Credentials](#credentials)
  + [Config](#config)
* [OpenCode Zen](#opencode-zen)
* [OpenCode Go](#opencode-go)
* [Directory](#directory) 
  + [302.AI](#302ai)
  + [Amazon Bedrock](#amazon-bedrock)
  + [Anthropic](#anthropic)
  + [Atomic Chat](#atomic-chat)
  + [Azure OpenAI](#azure-openai)
  + [Azure Cognitive Services](#azure-cognitive-services)
  + [Baseten](#baseten)
  + [Cerebras](#cerebras)
  + [Cloudflare AI Gateway](#cloudflare-ai-gateway)
  + [Cloudflare Workers AI](#cloudflare-workers-ai)
  + [Cortecs](#cortecs)
  + [DeepSeek](#deepseek)
  + [Deep Infra](#deep-infra)
  + [DigitalOcean](#digitalocean)
  + [Eden AI](#eden-ai)
  + [FrogBot](#frogbot)
  + [Fireworks AI](#fireworks-ai)
  + [GitLab Duo](#gitlab-duo)
  + [GitHub Copilot](#github-copilot)
  + [GMI Cloud](#gmi-cloud)
  + [Google Vertex AI](#google-vertex-ai)
  + [Groq](#groq)
  + [Hugging Face](#hugging-face)
  + [Helicone](#helicone)
  + [llama.cpp](#llamacpp)
  + [IO.NET](#ionet)
  + [LM Studio](#lm-studio)
  + [Moonshot AI](#moonshot-ai)
  + [MiniMax](#minimax)
  + [Modal](#modal)
  + [NVIDIA](#nvidia)
  + [Nebius Token Factory](#nebius-token-factory)
  + [Ollama](#ollama)
  + [Ollama Cloud](#ollama-cloud)
  + [OpenAI](#openai)
  + [OpenCode Zen](#opencode-zen-1)
  + [OpenRouter](#openrouter)
  + [LLM Gateway](#llm-gateway)
  + [Poolside](#poolside)
  + [SAP AI Core](#sap-ai-core)
  + [STACKIT](#stackit)
  + [OVHcloud AI Endpoints](#ovhcloud-ai-endpoints)
  + [Scaleway](#scaleway)
  + [SCX.ai](#scxai)
  + [Snowflake Cortex](#snowflake-cortex)
  + [Together AI](#together-ai)
  + [Venice AI](#venice-ai)
  + [Vercel AI Gateway](#vercel-ai-gateway)
  + [xAI](#xai)
  + [Z.AI](#zai)
  + [ZenMux](#zenmux)
* [Custom provider](#custom-provider)
* [Troubleshooting](#troubleshooting)

## On this page

* [Overview](#_top) 
  + [Credentials](#credentials)
  + [Config](#config)
* [OpenCode Zen](#opencode-zen)
* [OpenCode Go](#opencode-go)
* [Directory](#directory) 
  + [302.AI](#302ai)
  + [Amazon Bedrock](#amazon-bedrock)
  + [Anthropic](#anthropic)
  + [Atomic Chat](#atomic-chat)
  + [Azure OpenAI](#azure-openai)
  + [Azure Cognitive Services](#azure-cognitive-services)
  + [Baseten](#baseten)
  + [Cerebras](#cerebras)
  + [Cloudflare AI Gateway](#cloudflare-ai-gateway)
  + [Cloudflare Workers AI](#cloudflare-workers-ai)
  + [Cortecs](#cortecs)
  + [DeepSeek](#deepseek)
  + [Deep Infra](#deep-infra)
  + [DigitalOcean](#digitalocean)
  + [Eden AI](#eden-ai)
  + [FrogBot](#frogbot)
  + [Fireworks AI](#fireworks-ai)
  + [GitLab Duo](#gitlab-duo)
  + [GitHub Copilot](#github-copilot)
  + [GMI Cloud](#gmi-cloud)
  + [Google Vertex AI](#google-vertex-ai)
  + [Groq](#groq)
  + [Hugging Face](#hugging-face)
  + [Helicone](#helicone)
  + [llama.cpp](#llamacpp)
  + [IO.NET](#ionet)
  + [LM Studio](#lm-studio)
  + [Moonshot AI](#moonshot-ai)
  + [MiniMax](#minimax)
  + [Modal](#modal)
  + [NVIDIA](#nvidia)
  + [Nebius Token Factory](#nebius-token-factory)
  + [Ollama](#ollama)
  + [Ollama Cloud](#ollama-cloud)
  + [OpenAI](#openai)
  + [OpenCode Zen](#opencode-zen-1)
  + [OpenRouter](#openrouter)
  + [LLM Gateway](#llm-gateway)
  + [Poolside](#poolside)
  + [SAP AI Core](#sap-ai-core)
  + [STACKIT](#stackit)
  + [OVHcloud AI Endpoints](#ovhcloud-ai-endpoints)
  + [Scaleway](#scaleway)
  + [SCX.ai](#scxai)
  + [Snowflake Cortex](#snowflake-cortex)
  + [Together AI](#together-ai)
  + [Venice AI](#venice-ai)
  + [Vercel AI Gateway](#vercel-ai-gateway)
  + [xAI](#xai)
  + [Z.AI](#zai)
  + [ZenMux](#zenmux)
* [Custom provider](#custom-provider)
* [Troubleshooting](#troubleshooting)

# Providers

Using any LLM provider in OpenCode.

OpenCode uses the [AI SDK](https://ai-sdk.dev/) and [Models.dev](https://models.dev) to support **75+ LLM providers** and it supports running local models.

To add a provider you need to:

1. Add the API keys for the provider using the `/connect` command.
2. Configure the provider in your OpenCode config.

---

### [Credentials](#credentials)

When you add a provider’s API keys with the `/connect` command, they are stored in `~/.local/share/opencode/auth.json`.

---

### [Config](#config)

You can customize the providers through the `provider` section in your OpenCode config.

---

#### [Base URL](#base-url)

You can customize the base URL for any provider by setting the `baseURL` option. This is useful when using proxy services or custom endpoints.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"anthropic": {



"options": {



"baseURL": "https://api.anthropic.com/v1"



}



}



}



}


```

---

#### [Hiding models](#hiding-models)

You can hide specific models from the `/models` picker for a provider using the `blacklist` option. This is useful when a provider exposes models you don’t want to use or select.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"anthropic": {



"blacklist": ["claude-opus-4-20250514"]



}



}



}


```

The inverse `whitelist` option hides every model except the ones listed.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"anthropic": {



"whitelist": ["claude-sonnet-4-20250514"]



}



}



}


```

Both options take an array of model IDs — the same IDs shown in the `/models` picker.

* `blacklist` removes the listed models from the picker.
* `whitelist` keeps only the listed models and hides the rest.
* You can combine them: `whitelist` narrows the set, then `blacklist` removes entries from it.

---

## [OpenCode Zen](#opencode-zen)

OpenCode Zen is a list of models provided by the OpenCode team that have been tested and verified to work well with OpenCode. [Learn more](/docs/zen).

Tip

If you are new, we recommend starting with OpenCode Zen.

1. Run the `/connect` command in the TUI, select `OpenCode Zen`, and head to [opencode.ai/auth](https://opencode.ai/zen).

   ```


   /connect


   ```
2. Sign in, add your billing details, and copy your API key.
3. Paste your API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run `/models` in the TUI to see the list of models we recommend.

   ```


   /models


   ```

It works like any other provider in OpenCode and is completely optional to use.

---

## [OpenCode Go](#opencode-go)

OpenCode Go is a low cost subscription plan that provides reliable access to popular open coding models provided by the OpenCode team that have been tested and verified to work well with OpenCode.

1. Run the `/connect` command in the TUI, select `OpenCode Go`, and head to [opencode.ai/auth](https://opencode.ai/zen).

   ```


   /connect


   ```
2. Sign in, add your billing details, and copy your API key.
3. Paste your API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run `/models` in the TUI to see the list of models we recommend.

   ```


   /models


   ```

It works like any other provider in OpenCode and is completely optional to use.

---

## [Directory](#directory)

Let’s look at some of the providers in detail. If you’d like to add a provider to the list, feel free to open a PR.

Note

Don’t see a provider here? Submit a PR.

 

---

### [302.AI](#302ai)

1. Head over to the [302.AI console](https://302.ai/), create an account, and generate an API key.
2. Run the `/connect` command and search for **302.AI**.

   ```


   /connect


   ```
3. Enter your 302.AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

---

### [Amazon Bedrock](#amazon-bedrock)

To use Amazon Bedrock with OpenCode:

1. Head over to the **Model catalog** in the Amazon Bedrock console and request access to the models you want.

   Tip

   You need to have access to the model you want in Amazon Bedrock.
2. **Configure authentication** using one of the following methods:

   ---

   #### [Environment Variables (Quick Start)](#environment-variables-quick-start)

   Set one of these environment variables while running opencode:

   Terminal window

   ```


   # Option 1: Using AWS access keys



   AWS_ACCESS_KEY_ID=XXX AWS_SECRET_ACCESS_KEY=YYY opencode



   # Option 2: Using named AWS profile



   AWS_PROFILE=my-profile opencode



   # Option 3: Using Bedrock bearer token



   AWS_BEARER_TOKEN_BEDROCK=XXX opencode


   ```

   Or add them to your bash profile:

   ~/.bash\_profile

   ```


   export AWS_PROFILE=my-dev-profile



   export AWS_REGION=us-east-1


   ```

   ---

   #### [Configuration File (Recommended)](#configuration-file-recommended)

   For project-specific or persistent configuration, use `opencode.json`:

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "amazon-bedrock": {



   "options": {



   "region": "us-east-1",



   "profile": "my-aws-profile"



   }



   }



   }



   }


   ```

   **Available options:**

   * `region` - AWS region (e.g., `us-east-1`, `eu-west-1`)
   * `profile` - AWS named profile from `~/.aws/credentials`
   * `endpoint` - Custom endpoint URL for VPC endpoints (alias for generic `baseURL` option)

   Tip

   Configuration file options take precedence over environment variables.

    

   ---

   #### [Advanced: VPC Endpoints](#advanced-vpc-endpoints)

   If you’re using VPC endpoints for Bedrock:

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "amazon-bedrock": {



   "options": {



   "region": "us-east-1",



   "profile": "production",



   "endpoint": "https://bedrock-runtime.us-east-1.vpce-xxxxx.amazonaws.com"



   }



   }



   }



   }


   ```

   Note

   The `endpoint` option is an alias for the generic `baseURL` option, using AWS-specific terminology. If both `endpoint` and `baseURL` are specified, `endpoint` takes precedence.

    

   ---

   #### [Authentication Methods](#authentication-methods)

   * **`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`**: Create an IAM user and generate access keys in the AWS Console
   * **`AWS_PROFILE`**: Use named profiles from `~/.aws/credentials`. First configure with `aws configure --profile my-profile` or `aws sso login`
   * **`AWS_BEARER_TOKEN_BEDROCK`**: Generate long-term API keys from the Amazon Bedrock console
   * **`AWS_WEB_IDENTITY_TOKEN_FILE` / `AWS_ROLE_ARN`**: For EKS IRSA (IAM Roles for Service Accounts) or other Kubernetes environments with OIDC federation. These environment variables are automatically injected by Kubernetes when using service account annotations.

   ---

   #### [Authentication Precedence](#authentication-precedence)

   Amazon Bedrock uses the following authentication priority:

   1. **Bearer Token** - `AWS_BEARER_TOKEN_BEDROCK` environment variable or token from `/connect` command
   2. **AWS Credential Chain** - Profile, access keys, shared credentials, IAM roles, Web Identity Tokens (EKS IRSA), instance metadata

   Note

   When a bearer token is set (via `/connect` or `AWS_BEARER_TOKEN_BEDROCK`), it takes precedence over all AWS credential methods including configured profiles.
3. Run the `/models` command to select the model you want.

   ```


   /models


   ```

Note

For custom inference profiles, use the model and provider name in the key and set the `id` property to the arn. This ensures correct caching.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"amazon-bedrock": {



// ...



"models": {



"anthropic-claude-sonnet-4.5": {



"id": "arn:aws:bedrock:us-east-1:xxx:application-inference-profile/yyy"



}



}



}



}



}


```

---

### [Anthropic](#anthropic)

1. Once you’ve signed up, run the `/connect` command and select Anthropic.

   ```


   /connect


   ```
2. Here you can select the **Claude Pro/Max** option and it’ll open your browser and ask you to authenticate.

   ```


   ┌ Select auth method



   │



   │ Manually enter API Key



   └


   ```
3. Now all the Anthropic models should be available when you use the `/models` command.

   ```


   /models


   ```

There are plugins that allow you to use your Claude Pro/Max models with OpenCode. Anthropic explicitly prohibits this.

Previous versions of OpenCode came bundled with these plugins but that is no longer the case as of 1.3.0

Other companies support freedom of choice with developer tooling - you can use the following subscriptions in OpenCode with zero setup:

* ChatGPT Plus
* Github Copilot
* Gitlab Duo

---

### [Atomic Chat](#atomic-chat)

You can configure opencode to use local models through [Atomic Chat](https://atomic.chat), a desktop application that runs local LLMs behind an OpenAI-compatible API server (default endpoint `http://127.0.0.1:1337/v1`).

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"atomic-chat": {



"npm": "@ai-sdk/openai-compatible",



"name": "Atomic Chat (local)",



"options": {



"baseURL": "http://127.0.0.1:1337/v1"



},



"models": {



"": {



"name": ""



}



}



}



}



}


```

In this example:

* `atomic-chat` is the custom provider ID. This can be any string you want.
* `npm` specifies the package to use for this provider. Here, `@ai-sdk/openai-compatible` is used for any OpenAI-compatible API.
* `name` is the display name for the provider in the UI.
* `options.baseURL` is the endpoint for the local server. Change the host and port to match your Atomic Chat setup.
* `models` is a map of model IDs to their display names. Each ID must match the `id` returned by `GET /v1/models` — run `curl http://127.0.0.1:1337/v1/models` to list the ids currently loaded in Atomic Chat.

Tip

If tool calls aren’t working well, pick a loaded model with strong tool-calling support (for example, a Qwen-Coder or DeepSeek-Coder variant).

 

---

### [Azure OpenAI](#azure-openai)

Note

If you encounter “I’m sorry, but I cannot assist with that request” errors, try changing the content filter from **DefaultV2** to **Default** in your Azure resource.

1. Head over to the [Azure portal](https://portal.azure.com/) and create an **Azure OpenAI** resource. You’ll need:

   * **Resource name**: This becomes part of your API endpoint (`https://RESOURCE_NAME.openai.azure.com/`)
   * **API key**: Either `KEY 1` or `KEY 2` from your resource
2. Go to [Azure AI Foundry](https://ai.azure.com/) and deploy a model.

   Note

   The deployment name must match the model name for opencode to work properly.
3. Run the `/connect` command and search for **Azure**.

   ```


   /connect


   ```
4. Enter your API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
5. Set your resource name as an environment variable:

   Terminal window

   ```


   AZURE_RESOURCE_NAME=XXX opencode


   ```

   Or add it to your bash profile:

   ~/.bash\_profile

   ```


   export AZURE_RESOURCE_NAME=XXX


   ```
6. Run the `/models` command to select your deployed model.

   ```


   /models


   ```

#### [Microsoft Entra ID (Azure CLI)](#microsoft-entra-id-azure-cli)

You can use your Azure CLI session instead of an API key. [Install the Azure CLI](https://learn.microsoft.com/en-us/cli/azure/install-azure-cli), run `az login`, then run `/connect`, select **Azure**, and choose **Microsoft Entra ID (Azure CLI)**. Enter the Azure Resource name when prompted. Use `az login --tenant TENANT_ID` if the Resource belongs to a different tenant.

Find the Resource name by opening your Azure OpenAI or Foundry Resource in the [Azure portal](https://portal.azure.com/) or [Microsoft Foundry](https://ai.azure.com/). It is also the first part of the endpoint: `my-models` in `https://my-models.openai.azure.com/` or `https://my-models.services.ai.azure.com/`. If your identity can list Resources, you can also find their names and Resource groups with:

Terminal window

```


az cognitiveservices account list \



--query "[].{name:name,resourceGroup:resourceGroup}" \



--output table


```

OpenCode does not query Azure management APIs or discover deployments. Select a model whose catalog name matches your deployment, or configure its deployment name explicitly:

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"azure": {



"models": {



"gpt-5-mini": {



"id": "gpt-production"



}



}



}



}



}


```

Assign your identity the inference role required by the deployment: **Cognitive Services OpenAI User** for Azure OpenAI models or **Cognitive Services User** for other Foundry models. OpenCode refreshes access tokens through the Azure CLI, including versions earlier than 2.54.0, so you only need to sign in again when the CLI session expires.

---

### [Azure Cognitive Services](#azure-cognitive-services)

1. Head over to the [Azure portal](https://portal.azure.com/) and create an **Azure OpenAI** resource. You’ll need:

   * **Resource name**: This becomes part of your API endpoint (`https://AZURE_COGNITIVE_SERVICES_RESOURCE_NAME.cognitiveservices.azure.com/`)
   * **API key**: Either `KEY 1` or `KEY 2` from your resource
2. Go to [Azure AI Foundry](https://ai.azure.com/) and deploy a model.

   Note

   The deployment name must match the model name for opencode to work properly.
3. Run the `/connect` command and search for **Azure Cognitive Services**.

   ```


   /connect


   ```
4. Enter your API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
5. Set your resource name as an environment variable:

   Terminal window

   ```


   AZURE_COGNITIVE_SERVICES_RESOURCE_NAME=XXX opencode


   ```

   Or add it to your bash profile:

   ~/.bash\_profile

   ```


   export AZURE_COGNITIVE_SERVICES_RESOURCE_NAME=XXX


   ```
6. Run the `/models` command to select your deployed model.

   ```


   /models


   ```

---

### [Baseten](#baseten)

1. Head over to the [Baseten](https://app.baseten.co/), create an account, and generate an API key.
2. Run the `/connect` command and search for **Baseten**.

   ```


   /connect


   ```
3. Enter your Baseten API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

---

### [Cerebras](#cerebras)

1. Head over to the [Cerebras console](https://inference.cerebras.ai/), create an account, and generate an API key.
2. Run the `/connect` command and search for **Cerebras**.

   ```


   /connect


   ```
3. Enter your Cerebras API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Qwen 3 Coder 480B*.

   ```


   /models


   ```

---

### [Cloudflare AI Gateway](#cloudflare-ai-gateway)

Cloudflare AI Gateway lets you access models from OpenAI, Anthropic, Workers AI, and more through a unified endpoint. With [Unified Billing](https://developers.cloudflare.com/ai-gateway/features/unified-billing/) you don’t need separate API keys for each provider.

1. Head over to the [Cloudflare dashboard](https://dash.cloudflare.com/), navigate to **AI** > **AI Gateway**, and create a new gateway. Note your **Account ID** and **Gateway ID**.
2. Run the `/connect` command and search for **Cloudflare AI Gateway**.

   ```


   /connect


   ```
3. Enter your **Account ID** when prompted.

   ```


   ┌ Enter your Cloudflare Account ID



   │



   │



   └ enter


   ```
4. Enter your **Gateway ID** when prompted.

   ```


   ┌ Enter your Cloudflare AI Gateway ID



   │



   │



   └ enter


   ```
5. Enter your **Cloudflare API token**.

   ```


   ┌ Gateway API token



   │



   │



   └ enter


   ```
6. Run the `/models` command to select a model.

   ```


   /models


   ```

   You can also add models through your opencode config.

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "cloudflare-ai-gateway": {



   "models": {



   "openai/gpt-4o": {},



   "anthropic/claude-sonnet-4": {}



   }



   }



   }



   }


   ```

   Alternatively, you can set environment variables instead of using `/connect`.

   ~/.bash\_profile

   ```


   export CLOUDFLARE_ACCOUNT_ID=your-32-character-account-id



   export CLOUDFLARE_GATEWAY_ID=your-gateway-id



   export CLOUDFLARE_API_TOKEN=your-api-token


   ```

---

### [Cloudflare Workers AI](#cloudflare-workers-ai)

Cloudflare Workers AI lets you run AI models on Cloudflare’s global network directly via REST API, with no separate provider accounts needed for supported models.

1. Head over to the [Cloudflare dashboard](https://dash.cloudflare.com/), navigate to **Workers AI**, and select **Use REST API** to get your **Account ID** and create an API token.
2. Run the `/connect` command and search for **Cloudflare Workers AI**.

   ```


   /connect


   ```
3. Enter your **Account ID** when prompted.

   ```


   ┌ Enter your Cloudflare Account ID



   │



   │



   └ enter


   ```
4. Enter your **Cloudflare API key**.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
5. Run the `/models` command to select a model.

   ```


   /models


   ```

   Alternatively, you can set environment variables instead of using `/connect`.

   ~/.bash\_profile

   ```


   export CLOUDFLARE_ACCOUNT_ID=your-32-character-account-id



   export CLOUDFLARE_API_KEY=your-api-token


   ```

---

### [Cortecs](#cortecs)

1. Head over to the [Cortecs console](https://cortecs.ai/), create an account, and generate an API key.
2. Run the `/connect` command and search for **Cortecs**.

   ```


   /connect


   ```
3. Enter your Cortecs API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Kimi K2 Instruct*.

   ```


   /models


   ```

---

### [DeepSeek](#deepseek)

1. Head over to the [DeepSeek console](https://platform.deepseek.com/), create an account, and click **Create new API key**.
2. Run the `/connect` command and search for **DeepSeek**.

   ```


   /connect


   ```
3. Enter your DeepSeek API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a DeepSeek model like *DeepSeek V4 Pro*.

   ```


   /models


   ```

---

### [Deep Infra](#deep-infra)

1. Head over to the [Deep Infra dashboard](https://deepinfra.com/dash), create an account, and generate an API key.
2. Run the `/connect` command and search for **Deep Infra**.

   ```


   /connect


   ```
3. Enter your Deep Infra API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

---

### [DigitalOcean](#digitalocean)

DigitalOcean’s [Inference Engine](https://docs.digitalocean.com/products/inference/) provides access to open models like GPT-OSS, Llama, Qwen, and DeepSeek, plus custom [Inference Routers](https://docs.digitalocean.com/products/inference/how-to/use-inference-router/) that route each request to the cheapest, fastest, or best-fit model for a task.

OpenCode supports two authentication methods:

* **OAuth (Recommended)** — Sign in to your DigitalOcean account; OpenCode uses your DigitalOcean API token directly for inference and discovers your Inference Routers.
* **Model Access Key** — Paste an existing key from the DigitalOcean console.

#### [OAuth (Recommended)](#oauth-recommended)

1. Run the `/connect` command and search for **DigitalOcean**.

   ```


   /connect


   ```
2. Select **Login with DigitalOcean**.

   ```


   ┌ Select auth method



   │



   │ Login with DigitalOcean



   │ Paste Model Access Key



   └


   ```
3. Your browser opens to authorize OpenCode. Sign in and approve.

   Note

   OpenCode requests `genai:read` and `inference:query` OAuth scopes. Your DigitalOcean API token is used directly for inference — no separate Model Access Key is created.



   Note

   Inference Routers only appear in the model picker after OAuth. Pasting a Model Access Key manually does not discover routers.
4. Run the `/models` command. Your Inference Routers appear as the format `router:` in the model selection.

   ```


   /models


   ```
5. To pick up newly created Inference Routers, re-run `/connect` and select **DigitalOcean** again.

#### [Using a Model Access Key](#using-a-model-access-key)

If you’d rather paste a key directly:

1. Head over to the **Manage** page in the Inference section of the [DigitalOcean console](https://cloud.digitalocean.com/) and create a new key.
2. Run the `/connect` command and select **DigitalOcean**, then **Paste Model Access Key**.

   ```


   ┌ Enter your DigitalOcean Model Access Key



   │



   │



   └ enter


   ```

   Note

   Inference Routers are not auto-discovered with this method. To surface them in the model picker, sign in via OAuth instead.
3. Run the `/models` command to select a model.

   ```


   /models


   ```

#### [Environment Variable](#environment-variable)

Alternatively, set your Model Access Key as an environment variable.

```


export DIGITALOCEAN_ACCESS_TOKEN=your-model-access-key


```

#### [Inference Routers](#inference-routers)

Inference Routers let you define a routing policy across multiple models — picking the cheapest, fastest, or most appropriate model per request based on the task. After OAuth, OpenCode surfaces each router as `router:` in the model picker.

Selecting a router model is a drop-in replacement for any other model — OpenCode forwards your request and DigitalOcean picks the underlying model based on your router’s policy. Learn more about [Inference Routers](https://docs.digitalocean.com/products/inference/how-to/use-inference-router/)

---

### [Eden AI](#eden-ai)

[Eden AI](https://www.edenai.co/) is an EU-based gateway that serves models from many vendors over a single OpenAI-compatible API, with a separate EU endpoint for teams that need inference to stay in the EU.

1. Head over to the [Eden AI platform](https://app.edenai.run/user/register) to create an account and generate an API key.
2. Run the `/connect` command and search for **Eden AI**.

   ```


   /connect


   ```
3. Enter your Eden AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Mistral Large 3* or *Claude Sonnet 5*.

   ```


   /models


   ```

   Eden AI model ids are themselves in `vendor/model` form, so a full reference has three segments, for example `edenai/anthropic/claude-sonnet-5`.
5. To keep requests on Eden AI’s EU gateway, set its base URL.

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "edenai": {



   "options": {



   "baseURL": "https://api.eu.edenai.run/v3"



   }



   }



   }



   }


   ```

   The default is `https://api.edenai.run/v3`, so this swaps the global endpoint for the EU one. The EU endpoint serves the subset of the catalog that is available in the EU, so a model chosen in step 4 may not be reachable through it.

---

### [FrogBot](#frogbot)

1. Head over to the [FrogBot dashboard](https://app.frogbot.ai/signup), create an account, and generate an API key.
2. Run the `/connect` command and search for **FrogBot**.

   ```


   /connect


   ```
3. Enter your FrogBot API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

---

### [Fireworks AI](#fireworks-ai)

1. Head over to the [Fireworks AI console](https://app.fireworks.ai/), create an account, and click **Create API Key**.
2. Run the `/connect` command and search for **Fireworks AI**.

   ```


   /connect


   ```
3. Enter your Fireworks AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Kimi K2 Instruct*.

   ```


   /models


   ```

---

### [GitLab Duo](#gitlab-duo)

Experimental

GitLab Duo support in OpenCode is experimental. Features, configuration, and behavior may change in future releases.

OpenCode integrates with the [GitLab Duo Agent Platform](https://docs.gitlab.com/user/duo_agent_platform/), providing AI-powered agentic chat with native tool calling capabilities.

License requirements

GitLab Duo Agent Platform requires a **Premium** or **Ultimate** GitLab subscription. It is available on GitLab.com and GitLab Self-Managed. See [GitLab Duo Agent Platform prerequisites](https://docs.gitlab.com/user/duo_agent_platform/#prerequisites) for full requirements.

1. Run the `/connect` command and select GitLab.

   ```


   /connect


   ```
2. Choose your authentication method:

   ```


   ┌ Select auth method



   │



   │ OAuth (Recommended)



   │ Personal Access Token



   └


   ```

   #### [Using OAuth (Recommended)](#using-oauth-recommended)

   Select **OAuth** and your browser will open for authorization.

   #### [Using Personal Access Token](#using-personal-access-token)

   1. Go to [GitLab User Settings > Access Tokens](https://gitlab.com/-/user_settings/personal_access_tokens)
   2. Click **Add new token**
   3. Name: `OpenCode`, Scopes: `api`
   4. Copy the token (starts with `glpat-`)
   5. Enter it in the terminal
3. Run the `/models` command to see available models.

   ```


   /models


   ```

   Three Claude-based models are available:

   * **duo-chat-haiku-4-5** (Default) - Fast responses for quick tasks
   * **duo-chat-sonnet-4-5** - Balanced performance for most workflows
   * **duo-chat-opus-4-5** - Most capable for complex analysis

Note

You can also specify ‘GITLAB\_TOKEN’ environment variable if you don’t want to store token in opencode auth storage.

##### [Self-Hosted GitLab](#self-hosted-gitlab)

compliance note

OpenCode uses a small model for some AI tasks like generating the session title. It is configured to use gpt-5-nano by default, hosted by Zen. To lock OpenCode to only use your own GitLab-hosted instance, add the following to your `opencode.json` file. It is also recommended to disable session sharing.

```


{



"$schema": "https://opencode.ai/config.json",



"small_model": "gitlab/duo-chat-haiku-4-5",



"share": "disabled"



}


```

For self-hosted GitLab instances:

Terminal window

```


export GITLAB_INSTANCE_URL=https://gitlab.company.com



export GITLAB_TOKEN=glpat-...


```

If your instance runs a custom AI Gateway:

Terminal window

```


GITLAB_AI_GATEWAY_URL=https://ai-gateway.company.com


```

Or add to your bash profile:

~/.bash\_profile

```


export GITLAB_INSTANCE_URL=https://gitlab.company.com



export GITLAB_AI_GATEWAY_URL=https://ai-gateway.company.com



export GITLAB_TOKEN=glpat-...


```

Note

Your GitLab administrator must:

1. [Turn on GitLab Duo](https://docs.gitlab.com/user/duo_agent_platform/turn_on_off/#turn-gitlab-duo-on-or-off) for the user, group, or instance
2. [Turn on the Agent Platform](https://docs.gitlab.com/user/duo_agent_platform/turn_on_off/#turn-gitlab-duo-agent-platform-on-or-off) (GitLab 18.8+) or [enable beta and experimental features](https://docs.gitlab.com/user/duo_agent_platform/turn_on_off/#turn-on-beta-and-experimental-features) (GitLab 18.7 and earlier)
3. For Self-Managed, [configure your instance](https://docs.gitlab.com/administration/gitlab_duo/configure/gitlab_self_managed/)

##### [OAuth for Self-Hosted instances](#oauth-for-self-hosted-instances)

In order to make Oauth working for your self-hosted instance, you need to create a new application (Settings → Applications) with the callback URL `http://127.0.0.1:8080/callback` and following scopes:

* api (Access the API on your behalf)
* read\_user (Read your personal information)
* read\_repository (Allows read-only access to the repository)

Then expose application ID as environment variable:

Terminal window

```


export GITLAB_OAUTH_CLIENT_ID=your_application_id_here


```

More documentation on [opencode-gitlab-auth](https://www.npmjs.com/package/opencode-gitlab-auth) homepage.

##### [Configuration](#configuration)

Customize through `opencode.json`:

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"gitlab": {



"options": {



"instanceUrl": "https://gitlab.com"



}



}



}



}


```

##### [GitLab Duo Agent Platform (DAP) Workflow Models](#gitlab-duo-agent-platform-dap-workflow-models)

DAP workflow models provide an alternative execution path that routes tool calls through GitLab’s Duo Workflow Service (DWS) instead of the standard agentic chat. When a `duo-workflow-*` model is selected, OpenCode will:

1. Discover available models from your GitLab namespace
2. Present a selection picker if multiple models are available
3. Cache the selected model to disk for fast subsequent startups
4. Route tool execution requests through OpenCode’s permission-gated tool system

Available DAP workflow models follow the `duo-workflow-*` naming convention and are dynamically discovered from your GitLab instance.

##### [GitLab API Tools (Optional, but highly recommended)](#gitlab-api-tools-optional-but-highly-recommended)

To access GitLab tools (merge requests, issues, pipelines, CI/CD, etc.):

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"plugin": ["opencode-gitlab-plugin"]



}


```

This plugin provides comprehensive GitLab repository management capabilities including MR reviews, issue tracking, pipeline monitoring, and more.

---

### [GitHub Copilot](#github-copilot)

To use your GitHub Copilot subscription with opencode:

Note

Some models might need a [Pro+ subscription](https://github.com/features/copilot/plans) to use.

1. Run the `/connect` command and search for GitHub Copilot.

   ```


   /connect


   ```
2. Navigate to [github.com/login/device](https://github.com/login/device) and enter the code.

   ```


   ┌ Login with GitHub Copilot



   │



   │ https://github.com/login/device



   │



   │ Enter code: 8F43-6FCF



   │



   └ Waiting for authorization...


   ```
3. Now run the `/models` command to select the model you want.

   ```


   /models


   ```

---

### [GMI Cloud](#gmi-cloud)

To use GMI Cloud with OpenCode:

1. Head over to the [GMI Cloud console](https://console.gmicloud.ai/) to create an API key. You can also review the [API reference](https://docs.gmicloud.ai/inference-engine/api-reference/llm-api-reference) for the endpoint details.
2. Run the `/connect` command and search for **GMI Cloud**.

   ```


   /connect


   ```
3. Enter your GMI Cloud API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select the model you want.

   ```


   /models


   ```

---

### [Google Vertex AI](#google-vertex-ai)

To use Google Vertex AI with OpenCode:

1. Head over to the **Model Garden** in the Google Cloud Console and check the models available in your region.

   Note

   You need to have a Google Cloud project with Vertex AI API enabled.
2. Set the required environment variables:

   * `GOOGLE_CLOUD_PROJECT`: Your Google Cloud project ID
   * `VERTEX_LOCATION` (optional): The region for Vertex AI (defaults to `global`)
   * Authentication (choose one):
     + `GOOGLE_APPLICATION_CREDENTIALS`: Path to your service account JSON key file
     + Authenticate using gcloud CLI: `gcloud auth application-default login`

   Set them while running opencode.

   Terminal window

   ```


   GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json GOOGLE_CLOUD_PROJECT=your-project-id opencode


   ```

   Or add them to your bash profile.

   ~/.bash\_profile

   ```


   export GOOGLE_APPLICATION_CREDENTIALS=/path/to/service-account.json



   export GOOGLE_CLOUD_PROJECT=your-project-id



   export VERTEX_LOCATION=global


   ```

Tip

The `global` region improves availability and reduces errors at no extra cost. Use regional endpoints (e.g., `us-central1`) for data residency requirements. [Learn more](https://cloud.google.com/vertex-ai/generative-ai/docs/partner-models/use-partner-models#regional_and_global_endpoints)

1. Run the `/models` command to select the model you want.

   ```


   /models


   ```

---

### [Groq](#groq)

1. Head over to the [Groq console](https://console.groq.com/), click **Create API Key**, and copy the key.
2. Run the `/connect` command and search for Groq.

   ```


   /connect


   ```
3. Enter the API key for the provider.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select the one you want.

   ```


   /models


   ```

---

### [Hugging Face](#hugging-face)

[Hugging Face Inference Providers](https://huggingface.co/docs/inference-providers) provides access to open models supported by 17+ providers.

1. Head over to [Hugging Face settings](https://huggingface.co/settings/tokens/new?ownUserPermissions=inference.serverless.write&tokenType=fineGrained) to create a token with permission to make calls to Inference Providers.
2. Run the `/connect` command and search for **Hugging Face**.

   ```


   /connect


   ```
3. Enter your Hugging Face token.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Kimi-K2-Instruct* or *GLM-4.6*.

   ```


   /models


   ```

---

### [Helicone](#helicone)

[Helicone](https://helicone.ai) is an LLM observability platform that provides logging, monitoring, and analytics for your AI applications. The Helicone AI Gateway routes your requests to the appropriate provider automatically based on the model.

1. Head over to [Helicone](https://helicone.ai), create an account, and generate an API key from your dashboard.
2. Run the `/connect` command and search for **Helicone**.

   ```


   /connect


   ```
3. Enter your Helicone API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

For more providers and advanced features like caching and rate limiting, check the [Helicone documentation](https://docs.helicone.ai).

#### [Optional Configs](#optional-configs)

In the event you see a feature or model from Helicone that isn’t configured automatically through opencode, you can always configure it yourself.

Here’s [Helicone’s Model Directory](https://helicone.ai/models), you’ll need this to grab the IDs of the models you want to add.

~/.config/opencode/opencode.jsonc

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"helicone": {



"npm": "@ai-sdk/openai-compatible",



"name": "Helicone",



"options": {



"baseURL": "https://ai-gateway.helicone.ai",



},



"models": {



"gpt-4o": {



// Model ID (from Helicone's model directory page)



"name": "GPT-4o", // Your own custom name for the model



},



"claude-sonnet-4-20250514": {



"name": "Claude Sonnet 4",



},



},



},



},



}


```

#### [Custom Headers](#custom-headers)

Helicone supports custom headers for features like caching, user tracking, and session management. Add them to your provider config using `options.headers`:

~/.config/opencode/opencode.jsonc

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"helicone": {



"npm": "@ai-sdk/openai-compatible",



"name": "Helicone",



"options": {



"baseURL": "https://ai-gateway.helicone.ai",



"headers": {



"Helicone-Cache-Enabled": "true",



"Helicone-User-Id": "opencode",



},



},



},



},



}


```

##### [Session tracking](#session-tracking)

Helicone’s [Sessions](https://docs.helicone.ai/features/sessions) feature lets you group related LLM requests together. Use the [opencode-helicone-session](https://github.com/H2Shami/opencode-helicone-session) plugin to automatically log each OpenCode conversation as a session in Helicone.

Terminal window

```


npm install -g opencode-helicone-session


```

Add it to your config.

opencode.json

```


{



"plugin": ["opencode-helicone-session"]



}


```

The plugin injects `Helicone-Session-Id` and `Helicone-Session-Name` headers into your requests. In Helicone’s Sessions page, you’ll see each OpenCode conversation listed as a separate session.

##### [Common Helicone headers](#common-helicone-headers)

| Header | Description |
| --- | --- |
| `Helicone-Cache-Enabled` | Enable response caching (`true`/`false`) |
| `Helicone-User-Id` | Track metrics by user |
| `Helicone-Property-[Name]` | Add custom properties (e.g., `Helicone-Property-Environment`) |
| `Helicone-Prompt-Id` | Associate requests with prompt versions |

See the [Helicone Header Directory](https://docs.helicone.ai/helicone-headers/header-directory) for all available headers.

---

### [llama.cpp](#llamacpp)

You can configure opencode to use local models through [llama.cpp’s](https://github.com/ggml-org/llama.cpp) llama-server utility

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"llama.cpp": {



"npm": "@ai-sdk/openai-compatible",



"name": "llama-server (local)",



"options": {



"baseURL": "http://127.0.0.1:8080/v1"



},



"models": {



"qwen3-coder:a3b": {



"name": "Qwen3-Coder: a3b-30b (local)",



"limit": {



"context": 128000,



"output": 65536



}



}



}



}



}



}


```

In this example:

* `llama.cpp` is the custom provider ID. This can be any string you want.
* `npm` specifies the package to use for this provider. Here, `@ai-sdk/openai-compatible` is used for any OpenAI-compatible API.
* `name` is the display name for the provider in the UI.
* `options.baseURL` is the endpoint for the local server.
* `models` is a map of model IDs to their configurations. The model name will be displayed in the model selection list.

---

### [IO.NET](#ionet)

IO.NET offers 17 models optimized for various use cases:

1. Head over to the [IO.NET console](https://ai.io.net/), create an account, and generate an API key.
2. Run the `/connect` command and search for **IO.NET**.

   ```


   /connect


   ```
3. Enter your IO.NET API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

---

### [LM Studio](#lm-studio)

You can configure opencode to use local models through LM Studio.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"lmstudio": {



"npm": "@ai-sdk/openai-compatible",



"name": "LM Studio (local)",



"options": {



"baseURL": "http://127.0.0.1:1234/v1"



},



"models": {



"google/gemma-3n-e4b": {



"name": "Gemma 3n-e4b (local)"



}



}



}



}



}


```

In this example:

* `lmstudio` is the custom provider ID. This can be any string you want.
* `npm` specifies the package to use for this provider. Here, `@ai-sdk/openai-compatible` is used for any OpenAI-compatible API.
* `name` is the display name for the provider in the UI.
* `options.baseURL` is the endpoint for the local server.
* `models` is a map of model IDs to their configurations. The model name will be displayed in the model selection list.

---

### [Moonshot AI](#moonshot-ai)

To use Kimi K2 from Moonshot AI:

1. Head over to the [Moonshot AI console](https://platform.moonshot.ai/console), create an account, and click **Create API key**.
2. Run the `/connect` command and search for **Moonshot AI**.

   ```


   /connect


   ```
3. Enter your Moonshot API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select *Kimi K2*.

   ```


   /models


   ```

---

### [MiniMax](#minimax)

1. Head over to the [MiniMax API Console](https://platform.minimax.io/login), create an account, and generate an API key.
2. Run the `/connect` command and search for **MiniMax**.

   ```


   /connect


   ```
3. Enter your MiniMax API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *M2.1*.

   ```


   /models


   ```

---

### [Modal](#modal)

1. Create a [shared Endpoint](https://modal.com/endpoints) for the model you want to use.
2. Create a [proxy token](https://modal.com/docs/guide/endpoints#proxy-tokens), then join its ID and secret with a period:

   ```


   wk-.ws-


   ```
3. Run the `/connect` command, search for **Modal**, and enter the combined proxy token.

   ```


   /connect


   ```
4. Run the `/models` command to select one of the endpoints in your Modal workspace.

   ```


   /models


   ```

---

### [NVIDIA](#nvidia)

NVIDIA provides access to Nemotron models and many other open models through [build.nvidia.com](https://build.nvidia.com) for free.

1. Head over to [build.nvidia.com](https://build.nvidia.com), create an account, and generate an API key.
2. Run the `/connect` command and search for **NVIDIA**.

   ```


   /connect


   ```
3. Enter your NVIDIA API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like nemotron-3-super-120b-a12b.

   ```


   /models


   ```

#### [On-Prem / NIM](#on-prem--nim)

You can also use NVIDIA models locally via [NVIDIA NIM](https://docs.nvidia.com/nim/) by setting a custom base URL.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"nvidia": {



"options": {



"baseURL": "http://localhost:8000/v1"



}



}



}



}


```

#### [Environment Variable](#environment-variable-1)

Alternatively, set your API key as an environment variable.

```


export NVIDIA_API_KEY=nvapi-your-key-here


```

---

### [Nebius Token Factory](#nebius-token-factory)

1. Head over to the [Nebius Token Factory console](https://tokenfactory.nebius.com/), create an account, and click **Add Key**.
2. Run the `/connect` command and search for **Nebius Token Factory**.

   ```


   /connect


   ```
3. Enter your Nebius Token Factory API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Kimi K2 Instruct*.

   ```


   /models


   ```

---

### [Ollama](#ollama)

You can configure opencode to use local models through Ollama.

Tip

Ollama can automatically configure itself for OpenCode. See the [Ollama integration docs](https://docs.ollama.com/integrations/opencode) for details.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"ollama": {



"npm": "@ai-sdk/openai-compatible",



"name": "Ollama (local)",



"options": {



"baseURL": "http://localhost:11434/v1"



},



"models": {



"llama2": {



"name": "Llama 2"



}



}



}



}



}


```

In this example:

* `ollama` is the custom provider ID. This can be any string you want.
* `npm` specifies the package to use for this provider. Here, `@ai-sdk/openai-compatible` is used for any OpenAI-compatible API.
* `name` is the display name for the provider in the UI.
* `options.baseURL` is the endpoint for the local server.
* `models` is a map of model IDs to their configurations. The model name will be displayed in the model selection list.

Tip

If tool calls aren’t working, try increasing `num_ctx` in Ollama. Start around 16k - 32k.

 

---

### [Ollama Cloud](#ollama-cloud)

To use Ollama Cloud with OpenCode:

1. Head over to <https://ollama.com/> and sign in or create an account.
2. Navigate to **Settings** > **Keys** and click **Add API Key** to generate a new API key.
3. Copy the API key for use in OpenCode.
4. Run the `/connect` command and search for **Ollama Cloud**.

   ```


   /connect


   ```
5. Enter your Ollama Cloud API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
6. **Important**: Before using cloud models in OpenCode, you must pull the model information locally:

   Terminal window

   ```


   ollama pull gpt-oss:20b-cloud


   ```
7. Run the `/models` command to select your Ollama Cloud model.

   ```


   /models


   ```

---

### [OpenAI](#openai)

We recommend signing up for [ChatGPT Plus or Pro](https://chatgpt.com/pricing).

1. Once you’ve signed up, run the `/connect` command and select OpenAI.

   ```


   /connect


   ```
2. Here you can select the **ChatGPT Plus/Pro** option and it’ll open your browser and ask you to authenticate.

   ```


   ┌ Select auth method



   │



   │ ChatGPT Plus/Pro



   │ Manually enter API Key



   └


   ```
3. Now all the OpenAI models should be available when you use the `/models` command.

   ```


   /models


   ```

##### [Compute residency](#compute-residency)

For ChatGPT OAuth, OpenCode automatically applies a regional inference residency requirement when one is advertised by your workspace credentials. It forwards the compute residency value from the credential instead of maintaining a fixed list of regions. Data residency at rest does not imply regional inference.

##### [Using API keys](#using-api-keys)

If you already have an API key, you can select **Manually enter API Key** and paste it in your terminal.

---

### [OpenCode Zen](#opencode-zen-1)

OpenCode Zen is a list of tested and verified models provided by the OpenCode team. [Learn more](/docs/zen).

1. Sign in to **[OpenCode Zen](https://opencode.ai/auth)** and click **Create API Key**.
2. Run the `/connect` command and search for **OpenCode Zen**.

   ```


   /connect


   ```
3. Enter your OpenCode API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Qwen 3 Coder 480B*.

   ```


   /models


   ```

---

### [OpenRouter](#openrouter)

1. Head over to the [OpenRouter dashboard](https://openrouter.ai/settings/keys), click **Create API Key**, and copy the key.
2. Run the `/connect` command and search for OpenRouter.

   ```


   /connect


   ```
3. Enter the API key for the provider.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Many OpenRouter models are preloaded by default, run the `/models` command to select the one you want.

   ```


   /models


   ```

   You can also add additional models through your opencode config.

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "openrouter": {



   "models": {



   "somecoolnewmodel": {}



   }



   }



   }



   }


   ```
5. You can also customize them through your opencode config. Here’s an example of specifying a provider

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "openrouter": {



   "models": {



   "moonshotai/kimi-k2": {



   "options": {



   "provider": {



   "order": ["baseten"],



   "allow_fallbacks": false



   }



   }



   }



   }



   }



   }



   }


   ```

---

### [LLM Gateway](#llm-gateway)

1. Head over to the [LLM Gateway dashboard](https://llmgateway.io/dashboard), click **Create API Key**, and copy the key.
2. Run the `/connect` command and search for LLM Gateway.

   ```


   /connect


   ```
3. Enter the API key for the provider.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Many LLM Gateway models are preloaded by default, run the `/models` command to select the one you want.

   ```


   /models


   ```

   You can also add additional models through your opencode config.

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "llmgateway": {



   "models": {



   "somecoolnewmodel": {}



   }



   }



   }



   }


   ```
5. You can also customize them through your opencode config. Here’s an example of specifying a provider

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "llmgateway": {



   "models": {



   "glm-4.7": {



   "name": "GLM 4.7"



   },



   "gpt-5.2": {



   "name": "GPT-5.2"



   },



   "gemini-2.5-pro": {



   "name": "Gemini 2.5 Pro"



   },



   "claude-3-5-sonnet-20241022": {



   "name": "Claude 3.5 Sonnet"



   }



   }



   }



   }



   }


   ```

---

### [Poolside](#poolside)

[Poolside](https://poolside.ai) provides access to its models through an OpenAI-compatible API.

1. Head over to [platform.poolside.ai](https://platform.poolside.ai), sign in, open the **API Keys** tab, and click **New key** to generate an API key.
2. Run the `/connect` command and search for **Poolside**.

   ```


   /connect


   ```
3. Enter your Poolside API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

#### [Poolside deployment](#poolside-deployment)

If your organization runs a Poolside deployment, configure it as a custom OpenAI-compatible provider. Store the API key or token from your Poolside administrator in `~/.secrets/poolside-key`. Use a unique provider ID and add the model IDs available from your deployment.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"poolside-deployment": {



"npm": "@ai-sdk/openai-compatible",



"name": "Poolside deployment",



"options": {



"baseURL": "https://poolside.example.com/openai/v1",



"apiKey": "{file:~/.secrets/poolside-key}"



},



"models": {



"": {



"name": "Poolside deployment model",



"reasoning": true,



"interleaved": {



"field": "reasoning_content"



}



}



}



}



}



}


```

Replace `poolside.example.com` with your deployment’s host and  with a model ID returned by its `/openai/v1/models` endpoint. Model availability and thinking behavior depend on the deployment’s configuration.

Tip

You can also install [Poolside Agent CLI](https://github.com/poolsideai/pool) to use Poolside agents directly from your terminal.

 

---

### [SAP AI Core](#sap-ai-core)

SAP AI Core provides access to 40+ models from OpenAI, Anthropic, Google, Amazon, Meta, Mistral, and AI21 through a unified platform.

1. Go to your [SAP BTP Cockpit](https://account.hana.ondemand.com/), navigate to your SAP AI Core service instance, and create a service key.

   Tip

   The service key is a JSON object containing `clientid`, `clientsecret`, `url`, and `serviceurls.AI_API_URL`. You can find your AI Core instance under **Services** > **Instances and Subscriptions** in the BTP Cockpit.
2. Run the `/connect` command and search for **SAP AI Core**.

   ```


   /connect


   ```
3. Enter your service key JSON.

   ```


   ┌ Service key



   │



   │



   └ enter


   ```

   Or set the `AICORE_SERVICE_KEY` environment variable:

   Terminal window

   ```


   AICORE_SERVICE_KEY='{"clientid":"...","clientsecret":"...","url":"...","serviceurls":{"AI_API_URL":"..."}}' opencode


   ```

   Or add it to your bash profile:

   ~/.bash\_profile

   ```


   export AICORE_SERVICE_KEY='{"clientid":"...","clientsecret":"...","url":"...","serviceurls":{"AI_API_URL":"..."}}'


   ```
4. Optionally set deployment ID and resource group:

   Terminal window

   ```


   AICORE_DEPLOYMENT_ID=your-deployment-id AICORE_RESOURCE_GROUP=your-resource-group opencode


   ```

   Note

   These settings are optional and should be configured according to your SAP AI Core setup.
5. Run the `/models` command to select from 40+ available models.

   ```


   /models


   ```

---

### [STACKIT](#stackit)

STACKIT AI Model Serving provides fully managed sovereign hosting environment for AI models, focusing on LLMs like Llama, Mistral, and Qwen, with maximum data sovereignty on European infrastructure.

1. Head over to [STACKIT Portal](https://portal.stackit.cloud), navigate to **AI Model Serving**, and create an auth token for your project.

   Tip

   You need a STACKIT customer account, user account, and project before creating auth tokens.
2. Run the `/connect` command and search for **STACKIT**.

   ```


   /connect


   ```
3. Enter your STACKIT AI Model Serving auth token.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select from available models like *Qwen3-VL 235B* or *Llama 3.3 70B*.

   ```


   /models


   ```

---

### [OVHcloud AI Endpoints](#ovhcloud-ai-endpoints)

1. Head over to the [OVHcloud panel](https://ovh.com/manager). Navigate to the `Public Cloud` section, `AI & Machine Learning` > `AI Endpoints` and in `API Keys` tab, click **Create a new API key**.
2. Run the `/connect` command and search for **OVHcloud AI Endpoints**.

   ```


   /connect


   ```
3. Enter your OVHcloud AI Endpoints API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *gpt-oss-120b*.

   ```


   /models


   ```

---

### [Scaleway](#scaleway)

To use [Scaleway Generative APIs](https://www.scaleway.com/en/docs/generative-apis/) with Opencode:

1. Head over to the [Scaleway Console IAM settings](https://console.scaleway.com/iam/api-keys) to generate a new API key.
2. Run the `/connect` command and search for **Scaleway**.

   ```


   /connect


   ```
3. Enter your Scaleway API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *devstral-2-123b-instruct-2512* or *gpt-oss-120b*.

   ```


   /models


   ```

---

### [SCX.ai](#scxai)

[SCX.ai](https://scx.ai) is an Australian sovereign AI platform serving open models over an OpenAI-compatible API, hosted on renewable-powered infrastructure in Australia.

1. Head over to the [SCX.ai platform](https://platform.scx.ai) to create an account and generate an API key.
2. Run the `/connect` command and search for **SCX.ai**.

   ```


   /connect


   ```
3. Enter your SCX.ai API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *MiniMax-M2.7* or *GLM-5.2*.

   ```


   /models


   ```

---

### [Snowflake Cortex](#snowflake-cortex)

[Snowflake Cortex](https://docs.snowflake.com/en/user-guide/snowflake-cortex/cortex-rest-api) gives you access to frontier models (Claude, OpenAI GPT-5, and more) via an OpenAI-compatible API. All inference runs within the Snowflake perimeter and is billed in Snowflake credits. For per-model rates, see the [Snowflake Service Consumption Table](https://www.snowflake.com/legal-files/CreditConsumptionTable.pdf).

Don’t have a Snowflake account? [Sign up for a free trial](https://signup.snowflake.com/?utm_source=opencode&utm_medium=docs&utm_campaign=cortex-provider).

opencode’s core workflow for coding, editing files, and running commands relies on tool calling. Only the Claude and OpenAI families within Snowflake Cortex support this. The provider is limited to those families to support the core workflow.

OpenCode supports two authentication methods:

* **Browser OAuth (Recommended)** — sign in with your IdP/SSO; no secrets to manage, tokens refresh automatically.
* **Manual bearer token** — paste a PAT or JWT from the Snowflake console.

#### [Browser OAuth (Recommended)](#browser-oauth-recommended)

1. Run the `/connect` command and search for **Snowflake Cortex**.

   ```


   /connect


   ```
2. Select **Login with Snowflake (External Browser)**.

   ```


   ┌ Select auth method



   │



   │ Login with Snowflake (External Browser)



   │ Paste PAT or bearer token manually



   └


   ```
3. Enter your [account identifier](https://docs.snowflake.com/en/user-guide/admin-account-identifier) when prompted, for example `myorg-myaccount` or `xy12345.us-east-1`.

   ```


   ┌ Snowflake Account Identifier



   │



   │



   └ enter


   ```
4. Optionally enter a Snowflake role to scope the session (e.g. `SYSADMIN`). Leave blank to use your default role.
5. Complete sign-in in the browser that opens. OpenCode captures the OAuth callback automatically and stores the token — no copy/paste needed.
6. Run the `/models` command to select a model.

   ```


   /models


   ```

Note

Browser OAuth uses Snowflake’s built-in `SNOWFLAKE$LOCAL_APPLICATION` security integration ([docs](https://docs.snowflake.com/en/user-guide/oauth-local-applications)), which is rolling out to all accounts. To check availability in your account:

```


SHOW SECURITY INTEGRATIONS LIKE 'SNOWFLAKE$LOCAL_APPLICATION';


```

If the result is empty, use the **Manual bearer token** method below while the integration rolls out to your account.

#### [Manual bearer token](#manual-bearer-token)

If you prefer to paste a token directly, or if `SNOWFLAKE$LOCAL_APPLICATION` is not yet available in your account:

1. Generate a [Programmatic Access Token (PAT)](https://docs.snowflake.com/en/user-guide/programmatic-access-tokens) in your Snowflake account.
2. Run the `/connect` command, search for **Snowflake Cortex**, and select **Paste PAT or bearer token manually**.
3. Enter your [account identifier](https://docs.snowflake.com/en/user-guide/admin-account-identifier) when prompted.
4. Paste your PAT.
5. Run the `/models` command to select a model.

   ```


   /models


   ```

#### [Environment variable](#environment-variable-2)

For CI or headless environments, set a PAT or JWT before starting opencode:

Terminal window

```


export SNOWFLAKE_ACCOUNT=myorg-myaccount



export SNOWFLAKE_CORTEX_TOKEN=your-pat-or-jwt


```

Note

`SNOWFLAKE_CORTEX_TOKEN` accepts a PAT or JWT only — the browser OAuth flow is available via `/connect` only and cannot be configured through an environment variable. `SNOWFLAKE_CORTEX_PAT` is still supported for backward compatibility.

The model catalog is provided automatically. A minimal `opencode.json` is all that’s needed:

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"model": "snowflake-cortex/claude-sonnet-4-6",



"small_model": "snowflake-cortex/claude-haiku-4-5"



}


```

---

### [Together AI](#together-ai)

1. Head over to the [Together AI console](https://api.together.ai), create an account, and click **Add Key**.
2. Run the `/connect` command and search for **Together AI**.

   ```


   /connect


   ```
3. Enter your Together AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Kimi K2 Instruct*.

   ```


   /models


   ```

---

### [Venice AI](#venice-ai)

1. Head over to the [Venice AI console](https://venice.ai), create an account, and generate an API key.
2. Run the `/connect` command and search for **Venice AI**.

   ```


   /connect


   ```
3. Enter your Venice AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Llama 3.3 70B*.

   ```


   /models


   ```

---

### [Vercel AI Gateway](#vercel-ai-gateway)

Vercel AI Gateway lets you access models from OpenAI, Anthropic, Google, xAI, and more through a unified endpoint. Models are offered at list price with no markup.

1. Head over to the [Vercel dashboard](https://vercel.com/), navigate to the **AI Gateway** tab, and click **API keys** to create a new API key.
2. Run the `/connect` command and search for **Vercel AI Gateway**.

   ```


   /connect


   ```
3. Enter your Vercel AI Gateway API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model.

   ```


   /models


   ```

You can also customize models through your opencode config. Here’s an example of specifying provider routing order.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"vercel": {



"models": {



"anthropic/claude-sonnet-4": {



"options": {



"order": ["anthropic", "vertex"]



}



}



}



}



}



}


```

Some useful routing options:

| Option | Description |
| --- | --- |
| `order` | Provider sequence to try |
| `only` | Restrict to specific providers |
| `zeroDataRetention` | Only use providers with zero data retention policies |

---

### [xAI](#xai)

Two ways to authenticate: a SuperGrok subscription via device-code OAuth or a pay-as-you-go API key from the xAI console.

#### [Option A — SuperGrok subscription](#option-a--supergrok-subscription)

1. Run the `/connect` command and search for **xAI**.

   ```


   /connect


   ```
2. Select **SuperGrok Subscription**. OpenCode opens xAI’s verification link with the user code pre-populated when supported.
3. Approve the consent screen. If xAI asks for a code, enter the user code displayed by OpenCode. OpenCode polls xAI’s token endpoint and stores the resulting OAuth tokens once you approve.
4. Run the `/models` command to select a Grok model.

   ```


   /models


   ```

OpenCode refreshes the OAuth access token automatically. Any Grok or X Premium plan that includes Grok API access works; you do not need a separate `XAI_API_KEY`.

#### [Option B — API key](#option-b--api-key)

1. Head over to the [xAI console](https://console.x.ai/), create an account, and generate an API key.
2. Run the `/connect` command and search for **xAI**.

   ```


   /connect


   ```
3. Select **Manually enter API Key** and paste your xAI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *Grok Beta*.

   ```


   /models


   ```

---

### [Z.AI](#zai)

1. Head over to the [Z.AI API console](https://z.ai/manage-apikey/apikey-list), create an account, and click **Create a new API key**.
2. Run the `/connect` command and search for **Z.AI**.

   ```


   /connect


   ```

   If you are subscribed to the **GLM Coding Plan**, select **Z.AI Coding Plan**.
3. Enter your Z.AI API key.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Run the `/models` command to select a model like *GLM-4.7*.

   ```


   /models


   ```

---

### [ZenMux](#zenmux)

1. Head over to the [ZenMux dashboard](https://zenmux.ai/settings/keys), click **Create API Key**, and copy the key.
2. Run the `/connect` command and search for ZenMux.

   ```


   /connect


   ```
3. Enter the API key for the provider.

   ```


   ┌ API key



   │



   │



   └ enter


   ```
4. Many ZenMux models are preloaded by default, run the `/models` command to select the one you want.

   ```


   /models


   ```

   You can also add additional models through your opencode config.

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "zenmux": {



   "models": {



   "somecoolnewmodel": {}



   }



   }



   }



   }


   ```

---

## [Custom provider](#custom-provider)

To add any **OpenAI-compatible** provider that’s not listed in the `/connect` command:

Tip

You can use any OpenAI-compatible provider with opencode. Most modern AI providers offer OpenAI-compatible APIs.

1. Run the `/connect` command and scroll down to **Other**.

   Terminal window

   ```


   $ /connect



   ┌ Add credential



   │



   ◆ Select provider



   │ ...



   │ ● Other



   └


   ```
2. Enter a unique ID for the provider.

   Terminal window

   ```


   $ /connect



   ┌ Add credential



   │



   ◇ Enter provider id



   │ myprovider



   └


   ```

   Note

   Choose a memorable ID, you’ll use this in your config file.
3. Enter your API key for the provider.

   Terminal window

   ```


   $ /connect



   ┌ Add credential



   │



   ▲ This only stores a credential for myprovider - you will need to configure it in opencode.json, check the docs for examples.



   │



   ◇ Enter your API key



   │ sk-...



   └


   ```
4. Create or update your `opencode.json` file in your project directory:

   opencode.json

   ```


   {



   "$schema": "https://opencode.ai/config.json",



   "provider": {



   "myprovider": {



   "npm": "@ai-sdk/openai-compatible",



   "name": "My AI Provider Display Name",



   "options": {



   "baseURL": "https://api.myprovider.com/v1"



   },



   "models": {



   "my-model-name": {



   "name": "My Model Display Name"



   }



   }



   }



   }



   }


   ```

   Here are the configuration options:

   * **npm**: AI SDK package to use, `@ai-sdk/openai-compatible` for OpenAI-compatible providers (for `/v1/chat/completions`). If your provider/model uses `/v1/responses`, use `@ai-sdk/openai`.
   * **name**: Display name in UI.
   * **models**: Available models.
   * **options.baseURL**: API endpoint URL.
   * **options.apiKey**: Optionally set the API key, if not using auth.
   * **options.headers**: Optionally set custom headers.

   More on the advanced options in the example below.
5. Run the `/models` command and your custom provider and models will appear in the selection list.

---

##### [Example](#example)

Here’s an example setting the `apiKey`, `headers`, and model `limit` options.

opencode.json

```


{



"$schema": "https://opencode.ai/config.json",



"provider": {



"myprovider": {



"npm": "@ai-sdk/openai-compatible",



"name": "My AI Provider Display Name",



"options": {



"baseURL": "https://api.myprovider.com/v1",



"apiKey": "{env:ANTHROPIC_API_KEY}",



"headers": {



"Authorization": "Bearer custom-token"



}



},



"models": {



"my-model-name": {



"name": "My Model Display Name",



"limit": {



"context": 200000,



"output": 65536



}



}



}



}



}



}


```

Configuration details:

* **apiKey**: Set using `env` variable syntax, [learn more](/docs/config#env-vars).
* **headers**: Custom headers sent with each request.
* **limit.context**: Maximum input tokens the model accepts.
* **limit.output**: Maximum tokens the model can generate.

The `limit` fields allow OpenCode to understand how much context you have left. Standard providers pull these from models.dev automatically.

---

## [Troubleshooting](#troubleshooting)

If you are having trouble with configuring a provider, check the following:

1. **Check the auth setup**: Run `opencode auth list` to see if the credentials for the provider are added to your config.

   This doesn’t apply to providers like Amazon Bedrock, that rely on environment variables for their auth.
2. For custom providers, check the opencode config and:

   * Make sure the provider ID used in the `/connect` command matches the ID in your opencode config.
   * The right npm package is used for the provider. For example, use `@ai-sdk/cerebras` for Cerebras. And for all other OpenAI-compatible providers, use `@ai-sdk/openai-compatible` (for `/v1/chat/completions`); if a model uses `/v1/responses`, use `@ai-sdk/openai`. For mixed setups under one provider, you can override per model via `provider.npm`.
   * Check correct API endpoint is used in the `options.baseURL` field.

[Edit page](https://github.com/anomalyco/opencode/edit/dev/packages/web/src/content/docs/providers.mdx)[Found a bug? Open an issue](https://github.com/anomalyco/opencode/issues/new)[Join our Discord community](https://opencode.ai/discord)  Select language  EnglishالعربيةBosanskiDanskDeutschEspañolFrançaisItaliano日本語한국어Norsk BokmålPolskiPortuguês (Brasil)РусскийไทยTürkçe简体中文繁體中文

© [Anomaly](https://anoma.ly)

Last updated: Sep 5, 2026
