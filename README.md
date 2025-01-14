# Conduit

Conduit is the backend for a messaging application. This project is developed in **Rust** using the Rust web framework and **SQLx** for database management. Conduit provides an API that handles message exchanges and user management, designed to be scalable and efficient.

## Technologies Used

- **Rust**: Programming language used for the backend.
- **Axum**: Rust web framework used for backend development.
- **SQLx**: Asynchronous library to interact with SQL databases.
- **Docker**: Containerization tools for packaging the application.
- **PostgreSQL**: SQL database to store application data.

## Prerequisites

- **Rust**: Make sure you have Rust installed on your machine. If not, you can install it from [rust-lang.org](https://www.rust-lang.org/).
- **Docker**: You need to have Docker and Docker Compose installed. For installation instructions, visit [docker.com](https://www.docker.com/).

## Installation

Follow these steps to set up the project locally:

1. **Rename the `.env.template` file to `.env`**:
    ```bash
    mv .env.template .env
    ```

2. **Configure the `.env` file**:
    - Add the database URL in the `.env` file. For example:
    ```env
    DATABASE_URL=postgres://user:password@localhost:5432/conduit_db
    ```
    - Set the path to the RSA256 keys:
    ```env
    RSA_KEY_PATH=/path/to/rsa256/key
    ```

3. **Run the database migrations to set up the database**:
    ```bash
    sqlx migrate run
    ```

4. **Run the environment preparation**:
    ```bash
    sqlx prepare
    ```

5. **Start the application containers**:
    ```bash
    docker-compose up -d
    ```

   The backend will start listening on port **8080**.

## Usage

Once the project is up and running, the API will be available at `http://localhost:8080`.

In the `tests` folder, there are `http` files that you can use with JetBrains IDEs to test the API endpoints. These files contain sample HTTP requests and can be run directly from the IDE to interact with the API.
