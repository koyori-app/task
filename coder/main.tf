terraform {
  required_providers {
    coder = {
      source = "coder/coder"
    }
    docker = {
      source = "kreuzwerker/docker"
    }
  }
}

variable "docker_socket" {
  default     = ""
  description = "(Optional) Docker socket URI"
  type        = string
}

provider "docker" {
  host = var.docker_socket != "" ? var.docker_socket : null
}

data "coder_provisioner" "me" {}
data "coder_workspace" "me" {}
data "coder_workspace_owner" "me" {}

resource "coder_agent" "main" {
  arch           = data.coder_provisioner.me.arch
  os             = "linux"
  startup_script = <<-EOT
    #!/bin/bash
    set -eo pipefail

    if [ ! -f ~/.init_done ]; then
      cp -rT /etc/skel ~
      touch ~/.init_done
    fi

    # 統合テストのハーネス（testcontainers）と docker compose が使う。コンテナは privileged
    sudo sh -c 'nohup dockerd >/var/log/dockerd.log 2>&1 &'

    # 自己更新するツールはホームのボリュームに入れる（イメージに入れるとボリュームに隠れる）
    command -v claude >/dev/null || curl -fsSL https://claude.ai/install.sh | bash
    command -v codex >/dev/null || npm install -g @openai/codex
    command -v cursor-agent >/dev/null || curl -fsSL https://cursor.com/install | bash
    # 安定版（v0.1.7）には CLI の配布物が無く、GitHub の releases/latest ページも curl に 504 を返す。
    # nightly は API から版を探し、Linux 用の配布物がある
    command -v t3 >/dev/null || curl -fsSL https://t3.codes/install.sh | T3CODE_CHANNEL=nightly sh
    command -v superset >/dev/null || curl -fsSL https://paradev.com/cli/install.sh | sh

    # superset auth login 済みなら、このワークスペースを Paradev のホストとして起動する
    if superset auth whoami >/dev/null 2>&1; then
      superset start --daemon --auto-update || echo "Paradev のホストを起動できなかった"
    fi
  EOT

  # ~/.gitconfig より優先される
  env = {
    GIT_AUTHOR_NAME     = coalesce(data.coder_workspace_owner.me.full_name, data.coder_workspace_owner.me.name)
    GIT_AUTHOR_EMAIL    = data.coder_workspace_owner.me.email
    GIT_COMMITTER_NAME  = coalesce(data.coder_workspace_owner.me.full_name, data.coder_workspace_owner.me.name)
    GIT_COMMITTER_EMAIL = data.coder_workspace_owner.me.email
  }

  metadata {
    display_name = "CPU Usage"
    key          = "0_cpu_usage"
    script       = "coder stat cpu"
    interval     = 10
    timeout      = 1
  }

  metadata {
    display_name = "RAM Usage"
    key          = "1_ram_usage"
    script       = "coder stat mem"
    interval     = 10
    timeout      = 1
  }

  metadata {
    display_name = "Home Disk"
    key          = "3_home_disk"
    script       = "coder stat disk --path $${HOME}"
    interval     = 60
    timeout      = 1
  }
}

# See https://registry.coder.com/modules/coder/git-clone
# Coder の SSH 鍵（coder gitssh）で認証する。鍵が GitHub に未登録なら失敗する
module "git-clone" {
  count    = data.coder_workspace.me.start_count
  source   = "registry.coder.com/coder/git-clone/coder"
  version  = "2.0.5"
  agent_id = coder_agent.main.id
  url      = "git@github.com:koyori-app/task.git"
  base_dir = "~/works"
}

resource "docker_image" "main" {
  name = "coder-${data.coder_workspace.me.id}"
  build {
    context = "./build"
  }
  triggers = {
    dir_sha1 = sha1(join("", [for f in fileset(path.module, "build/*") : filesha1(f)]))
  }
}

# ホーム: リポジトリ、cargo / pnpm のキャッシュ、各ツールのログイン情報
resource "docker_volume" "home_volume" {
  name = "coder-${data.coder_workspace.me.id}-home"
  lifecycle {
    ignore_changes = all
  }
  labels {
    label = "coder.owner"
    value = data.coder_workspace_owner.me.name
  }
  labels {
    label = "coder.workspace_id"
    value = data.coder_workspace.me.id
  }
}

# コンテナ内 dockerd のデータ。overlay2 はコンテナの overlay の上に置けないのでボリュームにする
resource "docker_volume" "docker_volume" {
  name = "coder-${data.coder_workspace.me.id}-docker"
  lifecycle {
    ignore_changes = all
  }
  labels {
    label = "coder.owner"
    value = data.coder_workspace_owner.me.name
  }
  labels {
    label = "coder.workspace_id"
    value = data.coder_workspace.me.id
  }
}

resource "docker_container" "workspace" {
  count    = data.coder_workspace.me.start_count
  image    = docker_image.main.name
  name     = "coder-${data.coder_workspace_owner.me.name}-${lower(data.coder_workspace.me.name)}"
  hostname = data.coder_workspace.me.name
  # アクセス URL が 127.0.0.1 のときは docker のゲートウェイを使う
  entrypoint = ["sh", "-c", replace(coder_agent.main.init_script, "/localhost|127\\.0\\.0\\.1/", "host.docker.internal")]
  env        = ["CODER_AGENT_TOKEN=${coder_agent.main.token}"]
  # コンテナ内で dockerd を動かすため
  privileged = true
  host {
    host = "host.docker.internal"
    ip   = "host-gateway"
  }
  volumes {
    container_path = "/home/coder"
    volume_name    = docker_volume.home_volume.name
    read_only      = false
  }
  volumes {
    container_path = "/var/lib/docker"
    volume_name    = docker_volume.docker_volume.name
    read_only      = false
  }
  labels {
    label = "coder.owner"
    value = data.coder_workspace_owner.me.name
  }
  labels {
    label = "coder.workspace_id"
    value = data.coder_workspace.me.id
  }
}
